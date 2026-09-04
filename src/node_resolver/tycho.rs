//! Finding out where a Tycho validator is running.
//!
//! Tycho speaks nothing of ADNL. Its peers are QUIC endpoints named by an
//! ed25519 key, and every node publishes a record of the addresses it answers
//! on - signed by that key, renewed every few minutes, valid for an hour -
//! into the network's own DHT. A validator's peer id is the very key the
//! chain names it by, so the whole lookup is: ask the DHT for the record
//! stored under that key, and read the addresses out of it.
//!
//! What this replaces is a script on the validator host that asked the local
//! Tycho node which peers it happened to know. That answer was only ever as
//! good as one node's acquaintance, and it tied the map to a machine.

use super::dht::{Resolution, ResolvedAddress};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::debug;
use tycho_network::proto::dht::PeerValueKeyName;
use tycho_network::{Address, DhtClient, DhtConfig, DhtService, Network, PeerId, PeerInfo, Router};

/// How many times one validator is asked about before a pass gives up on it.
/// The same reasoning as the ADNL side: a search that finds nothing has not
/// necessarily been told there is nothing.
const LOOKUP_ATTEMPTS: usize = 3;
/// How long to wait before asking again.
const ATTEMPT_PAUSE: Duration = Duration::from_secs(2);
/// How long the DHT is given to settle after the bootstrap peers are added.
///
/// The client learns the network on its own in the background; asking it
/// about a validator in the same breath as starting it is asking an empty
/// routing table. A second or two is all it takes.
const WARMUP: Duration = Duration::from_secs(3);
/// How often this would publish a record of itself into the DHT.
///
/// Set to something that will never come around: this is a reader, and a
/// record pointing at a machine that answers no one is litter in someone
/// else's network.
const NEVER_ANNOUNCE: Duration = Duration::from_secs(365 * 24 * 60 * 60);

pub(super) struct TychoDhtResolver {
    /// Held for as long as the resolver lives. The DHT's background tasks
    /// hold only a weak reference to it, so dropping this closes the socket
    /// and ends them - no shutdown to remember.
    _network: Network,
    dht: DhtClient,
    local_addr: String,
    bootstrap_nodes: usize,
    lookup_timeout: Duration,
}

impl TychoDhtResolver {
    pub(super) async fn new(
        global_config_path: &Path,
        local_addr: &str,
        lookup_timeout: Duration,
    ) -> Result<Self> {
        let bootstrap_peers = read_bootstrap_peers(global_config_path)?;

        // A key of its own, made here and kept nowhere: this node's identity
        // matters only for the length of the process, and a key on disk would
        // be one more thing to guard for no gain.
        let secret = tycho_crypto::ed25519::SecretKey::from_bytes(rand::random());
        let keypair = tycho_crypto::ed25519::KeyPair::from(&secret);
        let local_id = PeerId::from(keypair.public_key);

        let (dht_tasks, dht_service) = DhtService::builder(local_id)
            .with_config(DhtConfig {
                local_info_announce_period: NEVER_ANNOUNCE,
                ..Default::default()
            })
            .build();

        let router = Router::builder().route(dht_service.clone()).build();
        let network = Network::builder()
            .with_private_key(secret.to_bytes())
            .build(local_addr, router)
            .with_context(|| format!("failed to open a Tycho socket on {local_addr}"))?;

        let bootstrap_nodes = dht_tasks
            .spawn(&network, &bootstrap_peers)
            .context("failed to greet the Tycho bootstrap peers")?;
        if bootstrap_nodes == 0 {
            return Err(anyhow!(
                "no usable bootstrap peers in {}",
                global_config_path.display()
            ));
        }

        let dht = dht_service.make_client(&network);
        Ok(Self {
            _network: network,
            dht,
            local_addr: local_addr.to_owned(),
            bootstrap_nodes,
            lookup_timeout,
        })
    }

    pub(super) fn local_addr(&self) -> &str {
        &self.local_addr
    }

    pub(super) fn bootstrap_nodes(&self) -> usize {
        self.bootstrap_nodes
    }

    /// Let the DHT find its feet before it is asked anything.
    pub(super) async fn warmup_network(&self) {
        sleep(WARMUP).await;
    }

    pub(super) async fn resolve(&self, peer_id: &str, now: u64) -> Resolution {
        let Some(peer_id) = parse_peer_id(peer_id) else {
            return Resolution::invalid_adnl(peer_id);
        };

        let mut last_error = None;
        for attempt in 1..=LOOKUP_ATTEMPTS {
            if attempt > 1 {
                sleep(ATTEMPT_PAUSE).await;
            }
            match timeout(self.lookup_timeout, self.find_peer_info(&peer_id)).await {
                Ok(Ok(address)) => return Resolution::resolved(address, now),
                Ok(Err(error)) => last_error = Some(error.to_string()),
                Err(_) => {
                    last_error = Some(format!(
                        "lookup timed out after {}s",
                        self.lookup_timeout.as_secs()
                    ));
                }
            }
        }

        Resolution::failed(last_error.unwrap_or_else(|| "no answer".to_owned()))
    }

    async fn find_peer_info(&self, peer_id: &PeerId) -> Result<ResolvedAddress> {
        let info = self
            .dht
            .entry(PeerValueKeyName::NodeInfo)
            .find_value::<PeerInfo>(peer_id)
            .await
            .map_err(|error| anyhow!("{error}"))?;

        // The DHT checks what it hands over, and this checks it again: the
        // record has to be the one asked for, signed by its owner, and not
        // out of date. It is about to be published on a map as the place a
        // named validator is running.
        if info.id != *peer_id {
            return Err(anyhow!("the DHT answered about {} instead", info.id));
        }
        if !info.verify(crate::timeutil::now_sec() as u32) {
            return Err(anyhow!("the record is expired or not properly signed"));
        }

        first_ip_address(&info).ok_or_else(|| {
            anyhow!(
                "the record lists {} address(es) and no IP among them",
                info.address_list.len()
            )
        })
    }
}

/// The first address a record gives that is an IP address.
///
/// A node may name itself by hostname instead. That is a fine way to be
/// reached and a useless one for a map, which needs somewhere to put a dot,
/// so those are passed over rather than resolved here.
fn first_ip_address(info: &PeerInfo) -> Option<ResolvedAddress> {
    info.address_list.iter().find_map(|address| match address {
        Address::Ip { ip, port } => Some(ResolvedAddress {
            ip: ip.to_string(),
            port: i32::from(*port),
            version: "quic".to_owned(),
        }),
        Address::Dns { hostname, port } => {
            debug!(%hostname, port, "a Tycho node named itself by hostname");
            None
        }
    })
}

fn parse_peer_id(value: &str) -> Option<PeerId> {
    let bytes = hex::decode(value).ok()?;
    Some(PeerId(<[u8; 32]>::try_from(bytes.as_slice()).ok()?))
}

/// The bootstrap half of a Tycho global config. The file also describes the
/// zerostate and the mempool, which a resolver has no use for.
#[derive(Debug, Deserialize)]
struct TychoGlobalConfig {
    #[serde(default)]
    bootstrap_peers: Vec<PeerInfo>,
}

fn read_bootstrap_peers(path: &Path) -> Result<Vec<PeerInfo>> {
    let body = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read global config {}", path.display()))?;
    let config: TychoGlobalConfig = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse global config {}", path.display()))?;
    if config.bootstrap_peers.is_empty() {
        return Err(anyhow!(
            "global config {} lists no bootstrap peers",
            path.display()
        ));
    }
    Ok(config.bootstrap_peers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_id_is_thirty_two_bytes_of_hex() {
        assert!(parse_peer_id(&"ab".repeat(32)).is_some());
        assert!(parse_peer_id(&"ab".repeat(31)).is_none());
        assert!(parse_peer_id("not hex").is_none());
        assert!(parse_peer_id("").is_none());
    }

    #[test]
    fn a_config_without_bootstrap_peers_is_not_a_config_to_start_from() {
        let dir = std::env::temp_dir().join(format!("tycho-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("global.json");

        std::fs::write(&path, r#"{"zerostate": {}}"#).unwrap();
        assert!(read_bootstrap_peers(&path).is_err());

        std::fs::write(&path, "{ not json").unwrap();
        assert!(read_bootstrap_peers(&path).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
