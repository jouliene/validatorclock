//! Turning a validator's ADNL address into the IP it is reachable at.
//!
//! Everscale publishes no directory of validator addresses; the only way to
//! learn where a validator is running is to ask the network's DHT for the
//! address its ADNL key is advertising. This is the one thing the map cannot
//! be drawn without, and the one thing this project used to get from a second
//! program of its own.
//!
//! Ported from `everscale_address_resolver`, whose behaviour is the reference:
//! ask the DHT, and when it does not answer, widen the search through the
//! bootstrap peers and then through every node the DHT has learned about.

use adnl::node::{AdnlNode, AdnlNodeConfig};
use adnl::{AddressSearchContext, DhtNode, DhtSearchPolicy};
use anyhow::{Context, Result, anyhow, bail};
use ever_block::{Ed25519KeyOption, KeyId, KeyOption, UInt256, base64_decode};
use futures::{StreamExt, stream};
use serde::Deserialize;
use std::convert::TryInto;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use ton_api::IntoBoxed;
use ton_api::ton::adnl::address::address::Udp;
use ton_api::ton::adnl::addresslist::AddressList as AdnlAddressList;
use ton_api::ton::dht::node::Node as DhtNodeConfig;
use ton_api::ton::pub_::publickey::Ed25519;
use tracing::debug;

/// Which of the local node's keys the DHT speaks with.
const DHT_KEY_TAG: usize = 1;
/// How wide a single DHT lookup searches before giving up.
const SEARCH_WIDTH: u8 = 5;
/// A ceiling on the nodes counted when reporting how much of the network the
/// DHT has learned of, so a large network cannot turn a log line into an
/// unbounded walk.
const MAX_KNOWN_NODES: usize = 10_000;
/// How many times one validator is asked about before a pass gives up on it.
///
/// A search that fails does not run out of time - it runs out of peers, and
/// says so in a fraction of a second. What makes it fail is the company it
/// keeps: a whole validator set going out at once means enough queries go
/// unanswered that the walk towards the node holding the value ends early.
/// Measured against TON's active set of 393, a pass loses four to eight
/// addresses and almost never the same ones twice, while those same addresses
/// asked for on their own answer at once. So a miss is worth asking again.
const LOOKUP_ATTEMPTS: usize = 3;
/// How long to wait before asking again, so the next try is not made in the
/// same crowd that lost the last one.
const ATTEMPT_PAUSE: Duration = Duration::from_secs(2);
/// How many bootstrap peers are greeted at once during warmup.
const WARMUP_CONCURRENCY: usize = 32;
/// How long one bootstrap peer has to answer the greeting.
///
/// A community bootstrap list is mostly history: of eighty entries in one, only
/// eighteen still answered. Greeted one at a time on a generous timeout, the
/// dead ones cost five minutes before any validator was looked up - longer than
/// the interval between passes. They are now greeted together and briefly,
/// because a peer that has not answered in a few seconds is not going to.
const WARMUP_TIMEOUT: Duration = Duration::from_secs(3);

/// Where a validator answers, as the DHT reported it.
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
pub(super) struct ResolvedAddress {
    pub(super) ip: String,
    pub(super) port: i32,
    pub(super) version: String,
}

/// What came of asking about one validator. Every outcome is recorded, not
/// just the successful one: a validator that cannot be found is a fact the
/// map has to show, and the reason belongs in the file with it.
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
pub(super) struct Resolution {
    pub(super) status: String,
    pub(super) addresses: Vec<ResolvedAddress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    /// When the DHT last confirmed this address.
    ///
    /// Carried on a fresh answer as well as a remembered one. Everything
    /// downstream - the location map, the history, how long a validator is
    /// given before it is called unfindable - measures age from here, and a
    /// reader that has to fall back on when the file was written cannot tell a
    /// node confirmed a moment ago from one last reached fifty minutes back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) confirmed_at: Option<u64>,
}

impl Resolution {
    pub(super) fn resolved(address: ResolvedAddress, confirmed_at: u64) -> Self {
        Self {
            status: "resolved".to_owned(),
            addresses: vec![address],
            error: None,
            confirmed_at: Some(confirmed_at),
        }
    }

    /// An address the DHT confirmed on an earlier pass and could not reach on
    /// this one. Named apart from `resolved` on purpose: it is older than the
    /// pass that reports it, and says so.
    pub(super) fn remembered(address: ResolvedAddress, confirmed_at: u64) -> Self {
        Self {
            status: "remembered".to_owned(),
            addresses: vec![address],
            error: None,
            confirmed_at: Some(confirmed_at),
        }
    }

    pub(super) fn failed(error: impl Into<String>) -> Self {
        Self {
            status: "failed".to_owned(),
            addresses: Vec::new(),
            error: Some(error.into()),
            confirmed_at: None,
        }
    }

    pub(super) fn missing_adnl() -> Self {
        Self {
            status: "missing_adnl".to_owned(),
            addresses: Vec::new(),
            error: Some("validator has no adnl_addr in active set".to_owned()),
            confirmed_at: None,
        }
    }

    pub(super) fn invalid_adnl(adnl_addr: &str) -> Self {
        Self {
            status: "invalid_adnl".to_owned(),
            addresses: Vec::new(),
            error: Some(format!("adnl_addr must be 32 bytes hex, got {adnl_addr}")),
            confirmed_at: None,
        }
    }

    #[cfg(test)]
    pub(super) fn failed_for_test(error: &str) -> Self {
        Self::failed(error)
    }

    /// Whether this validator has an address to put on the map, however it
    /// was arrived at.
    pub(super) fn has_address(&self) -> bool {
        !self.addresses.is_empty()
    }

    pub(super) fn is_resolved(&self) -> bool {
        self.status == "resolved" && !self.addresses.is_empty()
    }

    /// A lookup that went out and came back with nothing - as opposed to a
    /// validator that never had an address worth looking up.
    pub(super) fn is_failed(&self) -> bool {
        self.status == "failed"
    }
}

/// A validator address is 32 bytes of hex and nothing else; anything that is
/// not gets its own outcome rather than a failed lookup, so the file says
/// which validators the chain itself described badly.
pub(super) fn is_hex_32(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub(super) struct AdnlDhtResolver {
    /// Held for as long as the resolver lives. Dropping the handle does not
    /// close the socket - the node's own loops hold one too - so a resolver
    /// that is meant to end has to be told to, see `shutdown`.
    adnl: Arc<AdnlNode>,
    dht: Arc<DhtNode>,
    preset_nodes: Vec<Arc<KeyId>>,
    local_addr: String,
    lookup_timeout: Duration,
}

#[derive(Default)]
pub(super) struct NetworkWarmupStats {
    pub(super) checked: usize,
    pub(super) responsive: usize,
    pub(super) errors: usize,
    pub(super) known_nodes: usize,
}

impl AdnlDhtResolver {
    pub(super) async fn new(
        global_config_path: &Path,
        local_addr: &str,
        lookup_timeout: Duration,
    ) -> Result<Self> {
        let config = read_global_config(global_config_path)?;
        let dht_nodes = config.dht_node_configs()?;

        let (_, adnl_config) =
            AdnlNodeConfig::with_ip_address_and_private_key_tags(local_addr, vec![DHT_KEY_TAG])
                .context("failed to create local ADNL config")?;
        let adnl = AdnlNode::with_config(adnl_config)
            .await
            .context("failed to create local ADNL node")?;
        let dht = DhtNode::with_params(adnl.clone(), DHT_KEY_TAG, None)
            .context("failed to create DHT node")?;
        AdnlNode::start(&adnl, vec![dht.clone()])
            .await
            .context("failed to start ADNL node")?;

        let mut preset_nodes = Vec::new();
        for dht_node in &dht_nodes {
            if let Some(key) = dht
                .add_peer_to_network(dht_node, None)
                .context("failed to add DHT bootstrap peer")?
            {
                preset_nodes.push(key);
            }
        }

        if preset_nodes.is_empty() {
            bail!("bootstrap config has no valid DHT static nodes");
        }

        Ok(Self {
            adnl,
            dht,
            preset_nodes,
            local_addr: local_addr.to_owned(),
            lookup_timeout,
        })
    }

    /// Close the socket and stop the loops behind it.
    ///
    /// The resolver that runs the sweeps lives as long as the process and
    /// never needs this. The one built for a single round of second asks does:
    /// dropping it would leave its loops running and its socket open, and a
    /// pass every five minutes would go on opening more of them for ever.
    pub(super) async fn shutdown(self) {
        self.adnl.stop().await;
    }

    pub(super) fn bootstrap_nodes(&self) -> usize {
        self.preset_nodes.len()
    }

    pub(super) fn local_addr(&self) -> &str {
        &self.local_addr
    }

    /// Reach the bootstrap peers before a round of lookups, so the first
    /// validator asked about is not the one paying for a cold DHT.
    pub(super) async fn warmup_network(&self) -> NetworkWarmupStats {
        let responsive = stream::iter(self.preset_nodes.iter())
            .map(|node_key| async move {
                matches!(
                    timeout(
                        WARMUP_TIMEOUT,
                        self.dht.find_dht_nodes_in_network(node_key, None)
                    )
                    .await,
                    Ok(Ok(true))
                )
            })
            .buffer_unordered(WARMUP_CONCURRENCY)
            .filter(|answered| std::future::ready(*answered))
            .count()
            .await;

        NetworkWarmupStats {
            checked: self.preset_nodes.len(),
            responsive,
            errors: self.preset_nodes.len() - responsive,
            known_nodes: self
                .dht
                .get_known_nodes_of_network(MAX_KNOWN_NODES, None)
                .map(|nodes| nodes.len())
                .unwrap_or_default(),
        }
    }

    pub(super) async fn resolve(&self, adnl_addr: &str, now: u64) -> Resolution {
        match timeout(self.lookup_timeout, self.resolve_inner(adnl_addr)).await {
            Ok(Ok(address)) => Resolution::resolved(address, now),
            Ok(Err(error)) => Resolution::failed(error.to_string()),
            Err(_) => Resolution::failed(format!(
                "lookup timed out after {}s",
                self.lookup_timeout.as_secs()
            )),
        }
    }

    async fn resolve_inner(&self, adnl_addr: &str) -> Result<ResolvedAddress> {
        let adnl_key = hex::decode(adnl_addr).context("invalid adnl hex")?;
        let key_id = KeyId::from_data(
            adnl_key
                .as_slice()
                .try_into()
                .map_err(|_| anyhow!("adnl key must be 32 bytes"))?,
        );

        for attempt in 1..=LOOKUP_ATTEMPTS {
            if attempt > 1 {
                sleep(ATTEMPT_PAUSE).await;
            }
            // A search context of its own for each try. The context carries
            // the iterator of peers still worth asking, and a search that
            // found nothing is a search that emptied it: handing the same one
            // back means asking no one at all.
            let mut context = None;
            if let Some(address) = self.find_address(&key_id, &mut context).await {
                return Ok(address);
            }
        }

        bail!("the DHT had no address for this validator in {LOOKUP_ATTEMPTS} tries")
    }

    /// One attempt through the library's own search.
    ///
    /// A failure here is not the end of the search, and must not be returned
    /// as one. The library reports an address it cannot read as an error, and
    /// an answer we cannot read is "not here", not "stop looking" - propagating
    /// it would abandon the validator over one peer's malformed reply.
    async fn find_address(
        &self,
        key_id: &Arc<KeyId>,
        context: &mut Option<AddressSearchContext>,
    ) -> Option<ResolvedAddress> {
        match DhtNode::find_address_in_network_with_context(
            &self.dht,
            key_id,
            context,
            DhtSearchPolicy::FullSearch(SEARCH_WIDTH),
            None,
        )
        .await
        {
            Ok(Some((ip, _key))) => endpoint_from_display(&ip.to_string()).ok(),
            Ok(None) => None,
            Err(error) => {
                debug!(error = ?error, "the library could not read this address");
                None
            }
        }
    }
}

fn endpoint_from_display(value: &str) -> Result<ResolvedAddress> {
    let (ip, port) = value
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("DHT returned address without port: {value}"))?;
    Ok(ResolvedAddress {
        ip: ip.to_owned(),
        port: port
            .parse::<i32>()
            .with_context(|| format!("DHT returned invalid port in {value}"))?,
        version: "udp4".to_owned(),
    })
}

/// The bootstrap half of a chain's global config. Only the DHT static nodes
/// are read; the rest of the file describes things a resolver has no use for.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GlobalConfig {
    dht: DhtGlobalConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DhtGlobalConfig {
    static_nodes: DhtNodes,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DhtNodes {
    nodes: Vec<ConfigDhtNode>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigDhtNode {
    id: ConfigDhtNodeId,
    addr_list: ConfigAddressList,
    version: Option<i32>,
    signature: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigDhtNodeId {
    #[serde(alias = "@type")]
    type_node: Option<String>,
    key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigAddressList {
    addrs: Vec<ConfigAddress>,
    version: Option<i32>,
    reinit_date: Option<i32>,
    priority: Option<i32>,
    expire_at: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigAddress {
    ip: Option<i64>,
    port: Option<u16>,
}

impl GlobalConfig {
    /// The bootstrap peers, skipping any entry the file describes only
    /// partially. One unusable entry is not worth refusing the whole config
    /// over; no usable entry at all is, and the caller checks for that.
    fn dht_node_configs(&self) -> Result<Vec<DhtNodeConfig>> {
        let mut result = Vec::new();
        for dht_node in &self.dht.static_nodes.nodes {
            let key = dht_node.id.convert_key()?;
            let mut addrs = Vec::new();
            for addr in &dht_node.addr_list.addrs {
                let (Some(ip), Some(port)) = (addr.ip, addr.port) else {
                    continue;
                };
                addrs.push(
                    Udp {
                        ip: ip as i32,
                        port: port as i32,
                    }
                    .into_boxed(),
                );
            }

            let (
                Some(version),
                Some(reinit_date),
                Some(priority),
                Some(expire_at),
                Some(node_version),
                Some(signature),
            ) = (
                dht_node.addr_list.version,
                dht_node.addr_list.reinit_date,
                dht_node.addr_list.priority,
                dht_node.addr_list.expire_at,
                dht_node.version,
                dht_node.signature.as_ref(),
            )
            else {
                debug!("skipping a DHT bootstrap node the config describes only in part");
                continue;
            };

            result.push(DhtNodeConfig {
                id: Ed25519 {
                    key: UInt256::with_array(key.pub_key()?.try_into()?),
                }
                .into_boxed(),
                addr_list: AdnlAddressList {
                    addrs,
                    version,
                    reinit_date,
                    priority,
                    expire_at,
                },
                version: node_version,
                signature: base64_decode(signature)?,
            });
        }
        Ok(result)
    }
}

impl ConfigDhtNodeId {
    fn convert_key(&self) -> Result<Arc<dyn KeyOption>> {
        let type_node = self
            .type_node
            .as_deref()
            .ok_or_else(|| anyhow!("DHT node key type is missing"))?;
        if type_node != "pub.ed25519" {
            bail!("unsupported DHT node key type {type_node}");
        }

        let key = self
            .key
            .as_deref()
            .ok_or_else(|| anyhow!("DHT node public key is missing"))
            .and_then(base64_decode)?;
        let pub_key = key
            .get(..32)
            .ok_or_else(|| anyhow!("DHT node public key is shorter than 32 bytes"))?
            .try_into()?;
        Ok(Ed25519KeyOption::from_public_key(pub_key))
    }
}

fn read_global_config(path: &Path) -> Result<GlobalConfig> {
    let body = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read global config {}", path.display()))?;
    serde_json::from_str(&body)
        .with_context(|| format!("failed to parse global config {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_adnl_address_is_thirty_two_bytes_of_hex() {
        assert!(is_hex_32(&"a".repeat(64)));
        assert!(is_hex_32(
            "f9b2925adead7f441b32f510e472480a093c9514b6a1299612d3252fae9a97ab"
        ));
        assert!(!is_hex_32(&"a".repeat(63)));
        assert!(!is_hex_32(&"a".repeat(65)));
        assert!(!is_hex_32(&"z".repeat(64)));
        assert!(!is_hex_32(""));
    }

    #[test]
    fn an_address_from_the_dht_is_split_at_its_last_colon() {
        let address = endpoint_from_display("104.238.222.200:40100").unwrap();
        assert_eq!(address.ip, "104.238.222.200");
        assert_eq!(address.port, 40100);
        assert_eq!(address.version, "udp4");

        assert!(endpoint_from_display("104.238.222.200").is_err());
        assert!(endpoint_from_display("104.238.222.200:not-a-port").is_err());
    }

    #[test]
    fn a_bootstrap_node_missing_its_signature_is_skipped_not_fatal() {
        // Everything present except the signature. One unusable entry must not
        // cost the whole bootstrap list.
        let body = r#"{"dht":{"static_nodes":{"nodes":[
            {"id":{"@type":"pub.ed25519","key":"aGVsbG8gd29ybGQgaGVsbG8gd29ybGQgaGVsbG8gMTI="},
             "addr_list":{"addrs":[{"ip":1,"port":1}],"version":1,"reinit_date":1,
                          "priority":0,"expire_at":0},
             "version":1}
        ]}}}"#;
        let config: GlobalConfig = serde_json::from_str(body).unwrap();
        assert!(config.dht_node_configs().unwrap().is_empty());
    }
}
