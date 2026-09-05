use super::super::dto::ValidatorRoundData;
use super::super::graphql_client::GraphqlClient;
use super::super::round_stats::build_round_stats_response;
use super::super::util::now_sec;
use super::super::{ChainRoundStatsDto, ClockSnapshot, ElectionDto, RoundStatsPointDto};
use super::effective_validator_sets;
use super::election::election_from_elector_data;
use super::frozen::validator_round_data_from_elector_data;
use crate::config::ChainConfig;
use anyhow::{Context, Result, bail};
use minik2::ElectorData;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use tracing::debug;
use tycho_types::boc::Boc;
use tycho_types::cell::Cell;
use tycho_types::models::config::ElectionTimings;
use tycho_types::models::config::{BlockchainConfigParams, ValidatorSet};

const HEAD_QUERY: &str = "query($limit:Int){\
last:blocks(filter:{workchain_id:{eq:-1}},orderBy:{path:\"seq_no\",direction:DESC},limit:$limit){seq_no global_id}\
key:blocks(filter:{workchain_id:{eq:-1},key_block:{eq:true}},orderBy:{path:\"seq_no\",direction:DESC},limit:$limit){master{config{p0 p1}}}\
}";
const ACCOUNT_DATA_QUERY: &str = "query($ids:[String]){\
accounts(filter:{id:{in:$ids}},limit:2){id acc_type data}\
}";

pub(super) async fn fetch_chain_snapshot(
    chain: &ChainConfig,
    endpoint: &str,
) -> Result<ClockSnapshot> {
    let state = ChainState::fetch(endpoint).await?;
    let timings = state.election_timings()?;
    let observed_at = now_sec()?;
    let (current_set, next_set) = effective_validator_sets(
        state.current_validator_set()?,
        state.next_validator_set()?,
        observed_at,
    );
    let election = state.election();
    let validator_round_data = state.validator_round_data();

    Ok(super::snapshot::assemble_snapshot(
        super::snapshot::SnapshotParts {
            chain,
            endpoint,
            observed_at,
            global_id: state.global_id,
            seqno: state.seqno,
            timings,
            current_set,
            next_set,
            election,
            validator_round_data,
        },
    ))
}

pub(super) async fn fetch_chain_round_stats(
    chain: &ChainConfig,
    endpoint: &str,
    history_points: &[RoundStatsPointDto],
) -> Result<ChainRoundStatsDto> {
    let state = ChainState::fetch(endpoint).await?;
    let timings = state.election_timings()?;
    let observed_at = now_sec()?;
    let (current_set, _) = effective_validator_sets(
        state.current_validator_set()?,
        state.next_validator_set()?,
        observed_at,
    );
    let validator_round_data = validator_round_data_from_elector_data(&state.elector_data)?;

    Ok(build_round_stats_response(
        super::snapshot::chain_meta_with_rpc(chain, endpoint),
        observed_at,
        current_set.utime_since,
        timings.validators_elected_for,
        &validator_round_data,
        history_points,
    ))
}

struct ChainState {
    global_id: i32,
    seqno: u32,
    config: BlockchainConfigParams,
    elector_data: Cell,
}

impl ChainState {
    async fn fetch(endpoint: &str) -> Result<Self> {
        let client = GraphqlClient::new(endpoint)?;
        let head: HeadResponse = client
            .query("head", HEAD_QUERY, json!({ "limit": 1 }))
            .await?;
        let last_block = head
            .last
            .first()
            .context("GraphQL endpoint returned no masterchain blocks")?;
        let config_params = head
            .key
            .first()
            .and_then(|block| block.master.as_ref())
            .and_then(|master| master.config.as_ref())
            .context("GraphQL endpoint returned no key block config")?;
        let config_address = masterchain_address(
            config_params
                .p0
                .as_deref()
                .context("key block config has no param 0 (config address)")?,
            "config address",
        )?;
        let elector_address = masterchain_address(
            config_params
                .p1
                .as_deref()
                .context("key block config has no param 1 (elector address)")?,
            "elector address",
        )?;

        let accounts: AccountsResponse = client
            .query(
                "accounts",
                ACCOUNT_DATA_QUERY,
                json!({ "ids": [&config_address, &elector_address] }),
            )
            .await?;
        let config_data = accounts.active_account_data(&config_address, "config")?;
        let elector_data = accounts.active_account_data(&elector_address, "elector")?;

        Ok(Self {
            global_id: last_block.global_id,
            seqno: last_block.seq_no,
            config: blockchain_config_params(&config_data)?,
            elector_data: decode_account_data(&elector_data, "elector")?,
        })
    }

    fn election_timings(&self) -> Result<ElectionTimings> {
        self.config
            .get_election_timings()
            .context("failed to parse config param 15 (election timings)")
    }

    fn current_validator_set(&self) -> Result<ValidatorSet> {
        self.config
            .get_current_validator_set()
            .context("failed to parse config param 34/35 (current validator set)")
    }

    fn next_validator_set(&self) -> Result<Option<ValidatorSet>> {
        self.config
            .get_next_validator_set()
            .context("failed to parse config param 36/37 (next validator set)")
    }

    fn election(&self) -> ElectionDto {
        match ElectorData::parse(&self.elector_data) {
            Ok(data) => election_from_elector_data(&data),
            Err(error) => {
                debug!(error = ?error, "failed to parse GraphQL elector current election");
                ElectionDto::default()
            }
        }
    }

    fn validator_round_data(&self) -> HashMap<u32, ValidatorRoundData> {
        match validator_round_data_from_elector_data(&self.elector_data) {
            Ok(round_data) => round_data,
            Err(error) => {
                debug!(error = ?error, "failed to parse GraphQL elector frozen round data");
                HashMap::new()
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct HeadResponse {
    #[serde(default)]
    last: Vec<HeadBlock>,
    #[serde(default)]
    key: Vec<KeyBlock>,
}

#[derive(Debug, Deserialize)]
struct HeadBlock {
    seq_no: u32,
    global_id: i32,
}

#[derive(Debug, Deserialize)]
struct KeyBlock {
    master: Option<KeyBlockMaster>,
}

#[derive(Debug, Deserialize)]
struct KeyBlockMaster {
    config: Option<KeyBlockConfig>,
}

#[derive(Debug, Deserialize)]
struct KeyBlockConfig {
    p0: Option<String>,
    p1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountsResponse {
    #[serde(default)]
    accounts: Vec<AccountEntry>,
}

impl AccountsResponse {
    fn active_account_data(&self, address: &str, name: &str) -> Result<String> {
        let account = self
            .accounts
            .iter()
            .find(|account| account.id.eq_ignore_ascii_case(address))
            .with_context(|| format!("{name} account `{address}` not found"))?;
        if account.acc_type != ACCOUNT_TYPE_ACTIVE {
            bail!("{name} account `{address}` is not active");
        }

        account
            .data
            .as_deref()
            .filter(|data| !data.is_empty())
            .map(str::to_owned)
            .with_context(|| format!("{name} account `{address}` has no data"))
    }
}

#[derive(Debug, Deserialize)]
struct AccountEntry {
    id: String,
    acc_type: u8,
    data: Option<String>,
}

const ACCOUNT_TYPE_ACTIVE: u8 = 1;

fn blockchain_config_params(config_data: &str) -> Result<BlockchainConfigParams> {
    let data = decode_account_data(config_data, "config")?;
    let params_root = data
        .reference_cloned(0)
        .context("config account data has no config params dictionary")?;
    Ok(BlockchainConfigParams::from_raw(params_root))
}

fn decode_account_data(data: &str, name: &str) -> Result<Cell> {
    Boc::decode_base64(data).with_context(|| format!("failed to decode {name} account data"))
}

fn masterchain_address(hash: &str, name: &str) -> Result<String> {
    let hash = hash.trim().trim_start_matches("0x");
    if hash.len() != 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("key block config has an invalid {name} `{hash}`");
    }

    Ok(format!("-1:{}", hash.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts_response(id: &str, acc_type: u8, data: Option<&str>) -> AccountsResponse {
        serde_json::from_value(json!({
            "accounts": [{ "id": id, "acc_type": acc_type, "data": data }]
        }))
        .unwrap()
    }

    #[test]
    fn builds_masterchain_address_from_config_hash() {
        let hash = "3".repeat(64);

        assert_eq!(
            masterchain_address(&hash, "elector address").unwrap(),
            format!("-1:{hash}")
        );
        assert_eq!(
            masterchain_address(&format!("0x{}", "A".repeat(64)), "elector address").unwrap(),
            format!("-1:{}", "a".repeat(64))
        );
        assert!(masterchain_address("3333", "elector address").is_err());
        assert!(masterchain_address(&"z".repeat(64), "elector address").is_err());
    }

    #[test]
    fn reads_active_account_data() {
        let address = format!("-1:{}", "3".repeat(64));
        let response = accounts_response(&address, 1, Some("te6"));

        assert_eq!(
            response.active_account_data(&address, "elector").unwrap(),
            "te6"
        );
    }

    #[test]
    fn rejects_inactive_or_empty_accounts() {
        let address = format!("-1:{}", "3".repeat(64));

        let frozen = accounts_response(&address, 2, Some("te6"));
        let error = frozen
            .active_account_data(&address, "elector")
            .unwrap_err()
            .to_string();
        assert!(error.contains("is not active"));

        let empty = accounts_response(&address, 1, None);
        let error = empty
            .active_account_data(&address, "elector")
            .unwrap_err()
            .to_string();
        assert!(error.contains("has no data"));

        let missing = accounts_response(&format!("0:{}", "1".repeat(64)), 1, Some("te6"));
        let error = missing
            .active_account_data(&address, "elector")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not found"));
    }

    #[tokio::test]
    #[ignore = "requires a public GraphQL endpoint"]
    async fn live_graphql_endpoint_returns_snapshot() -> Result<()> {
        let endpoint = std::env::var("VALIDATORCLOCK_LIVE_GRAPHQL_ENDPOINT")
            .expect("set VALIDATORCLOCK_LIVE_GRAPHQL_ENDPOINT to run this test");
        let chain = ChainConfig {
            id: "everscale".to_owned(),
            name: "Everscale".to_owned(),
            rpc: endpoint.clone(),
            rpc_fallbacks: Vec::new(),
            color: "#6347F5".to_owned(),
            token_symbol: "EVER".to_owned(),
            rpc_label: None,
        };

        let snapshot = fetch_chain_snapshot(&chain, &endpoint).await?;

        assert!(snapshot.seqno > 0);
        assert!(snapshot.global_id != 0);
        assert!(snapshot.params15.validators_elected_for > 0);
        assert!(snapshot.current_set.total > 0);
        assert!(u64::from(snapshot.current_set.utime_since) < snapshot.fetched_at);
        assert!(u64::from(snapshot.current_set.utime_until) > snapshot.fetched_at);
        assert!(
            snapshot
                .current_set
                .validators
                .iter()
                .all(|validator| validator.wallet.is_some()),
            "frozen elector round data did not resolve validator wallets"
        );
        assert!(snapshot.previous_set.is_some());

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a public GraphQL endpoint"]
    async fn live_graphql_endpoint_returns_round_stats() -> Result<()> {
        let endpoint = std::env::var("VALIDATORCLOCK_LIVE_GRAPHQL_ENDPOINT")
            .expect("set VALIDATORCLOCK_LIVE_GRAPHQL_ENDPOINT to run this test");
        let chain = ChainConfig {
            id: "everscale".to_owned(),
            name: "Everscale".to_owned(),
            rpc: endpoint.clone(),
            rpc_fallbacks: Vec::new(),
            color: "#6347F5".to_owned(),
            token_symbol: "EVER".to_owned(),
            rpc_label: None,
        };

        let stats = fetch_chain_round_stats(&chain, &endpoint, &[]).await?;

        assert!(stats.has_round_data());

        Ok(())
    }

    #[test]
    fn parses_head_response() {
        let head: HeadResponse = serde_json::from_value(json!({
            "last": [{ "seq_no": 42, "global_id": 42 }],
            "key": [{ "master": { "config": { "p0": "55", "p1": "33" } } }]
        }))
        .unwrap();

        assert_eq!(head.last[0].seq_no, 42);
        assert_eq!(head.last[0].global_id, 42);
        assert_eq!(
            head.key[0]
                .master
                .as_ref()
                .and_then(|master| master.config.as_ref())
                .and_then(|config| config.p1.as_deref()),
            Some("33")
        );
    }
}
