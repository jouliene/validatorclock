use super::*;
use crate::config::AppConfig;
use axum::Router;
use axum::extract::{Json, State};
use axum::routing::{get, post};
use minik2::{HashBytes, ValidatorSet};
use serde_json::{Value, json};
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tycho_types::boc::{Boc, BocRepr};
use tycho_types::cell::{CellBuilder, Store};
use tycho_types::models::config::{ElectionTimings, ValidatorDescription};

#[tokio::test]
async fn broxus_failure_uses_toncenter_fallback_and_enrichment() -> Result<()> {
    let mock = Arc::new(MockTonCenter::new()?);
    let endpoint = spawn_mock_toncenter(Arc::clone(&mock)).await?;
    let state_dir = test_state_dir()?;
    let config = Arc::new(AppConfig {
        refresh_timeout_seconds: 15,
        ..AppConfig::for_test_in(
            &state_dir,
            vec![ChainConfig {
                id: "ton".to_owned(),
                name: "TON".to_owned(),
                rpc: "http://127.0.0.1:9/broxus-disabled".to_owned(),
                rpc_fallbacks: vec![format!("{endpoint}/api/v2/jsonRPC")],
                color: "#0098ea".to_owned(),
                token_symbol: "TON".to_owned(),
                rpc_label: None,
            }],
        )
    });
    let state = Arc::new(AppState::new(config));

    let snapshot = get_chain_snapshot_cached_first(Arc::clone(&state), "ton", true).await?;

    // The fallback served this - `selected_endpoint` below is the proof. What
    // reaches the reader is a working clock and no word about the machinery:
    // which endpoint answered is the operator's business, and one of them may
    // be an address on the server itself.
    assert_eq!(
        snapshot.warning, None,
        "the reader should not be told that a fallback RPC answered"
    );
    assert!(
        snapshot
            .selected_endpoint
            .as_deref()
            .is_some_and(|endpoint| endpoint.ends_with("/api/v2/jsonRPC"))
    );
    assert_eq!(snapshot.current_set.total, 1);
    assert_eq!(snapshot.election.candidates.len(), 1);
    let candidate = &snapshot.election.candidates[0];
    assert_eq!(candidate.wallet, mock.wallet_address.as_str());
    assert!(candidate.contract_type_hash.is_some());
    assert_eq!(mock.account_states_requests.load(Ordering::SeqCst), 1);

    Ok(())
}

#[tokio::test]
async fn toncenter_primary_fetches_snapshot_and_enrichment() -> Result<()> {
    let mock = Arc::new(MockTonCenter::new()?);
    let endpoint = spawn_mock_toncenter(Arc::clone(&mock)).await?;
    let state_dir = test_state_dir()?;
    let toncenter_endpoint = format!("{endpoint}/api/v2/jsonRPC");
    let config = Arc::new(AppConfig {
        refresh_timeout_seconds: 15,
        ..AppConfig::for_test_in(
            &state_dir,
            vec![ChainConfig {
                id: "ton".to_owned(),
                name: "TON".to_owned(),
                rpc: toncenter_endpoint.clone(),
                rpc_fallbacks: vec!["http://127.0.0.1:9/broxus-disabled".to_owned()],
                color: "#0098ea".to_owned(),
                token_symbol: "TON".to_owned(),
                rpc_label: None,
            }],
        )
    });
    let state = Arc::new(AppState::new(config));

    let snapshot = get_chain_snapshot_cached_first(Arc::clone(&state), "ton", true).await?;

    assert!(snapshot.warning.is_none());
    assert_eq!(
        snapshot.selected_endpoint.as_deref(),
        Some(toncenter_endpoint.as_str())
    );
    assert_eq!(snapshot.seqno, 12345);
    assert_eq!(snapshot.current_set.total, 1);
    assert_eq!(snapshot.election.candidates.len(), 1);
    assert_eq!(mock.account_states_requests.load(Ordering::SeqCst), 1);

    Ok(())
}

#[test]
fn degraded_refresh_detects_missing_active_round_data() {
    let cached = crate::chain::test_clock_snapshot("everscale");
    let mut refreshed = cached.clone();
    strip_active_round_data(&mut refreshed);

    assert_eq!(
        degraded_refresh_reason(&refreshed, &cached).as_deref(),
        Some("active validator round data is missing")
    );
}

#[test]
fn degraded_refresh_detects_missing_active_validator_details() {
    let cached = crate::chain::test_clock_snapshot("everscale");
    let mut refreshed = cached.clone();
    for validator in &mut refreshed.current_set.validators {
        validator.wallet = None;
        validator.source = None;
        validator.contract_type = None;
        validator.contract_type_hash = None;
        validator.stake = None;
    }

    assert_eq!(
        degraded_refresh_reason(&refreshed, &cached).as_deref(),
        Some("active validator round data is missing")
    );
}

#[test]
fn degraded_refresh_detects_missing_election_candidates_inside_window() {
    let mut cached = crate::chain::test_clock_snapshot("everscale");
    cached.current_set.utime_since = 1_000;
    cached.current_set.utime_until = 2_000;
    cached.fetched_at = 1_600;
    cached.params15.elections_start_before = 500;
    cached.params15.elections_end_before = 100;
    cached.election.candidates.push(test_candidate());

    let mut refreshed = cached.clone();
    refreshed.election.candidates.clear();

    assert_eq!(
        degraded_refresh_reason(&refreshed, &cached).as_deref(),
        Some("election candidates are missing during the election window")
    );
}

#[test]
fn degraded_refresh_allows_empty_election_candidates_after_window() {
    let mut cached = crate::chain::test_clock_snapshot("everscale");
    cached.current_set.utime_since = 1_000;
    cached.current_set.utime_until = 2_000;
    cached.fetched_at = 1_950;
    cached.params15.elections_start_before = 500;
    cached.params15.elections_end_before = 100;
    cached.election.candidates.push(test_candidate());

    let mut refreshed = cached.clone();
    refreshed.election.candidates.clear();

    assert!(degraded_refresh_reason(&refreshed, &cached).is_none());
}

#[test]
fn degraded_refresh_does_not_reuse_cache_for_new_active_round() {
    let cached = crate::chain::test_clock_snapshot("everscale");
    let mut refreshed = cached.clone();
    refreshed.current_set.round_id += 1;
    refreshed.current_set.utime_since += 65_536;
    refreshed.current_set.utime_until += 65_536;
    strip_active_round_data(&mut refreshed);

    assert!(degraded_refresh_reason(&refreshed, &cached).is_none());
}

#[test]
fn degraded_refresh_keeps_cache_when_active_round_moves_backwards() {
    let mut cached = crate::chain::test_clock_snapshot("everscale");
    cached.current_set.round_id = 27_263;
    cached.current_set.utime_since = 27_263 * 65_536;
    cached.current_set.utime_until = 27_264 * 65_536;

    let mut refreshed = cached.clone();
    refreshed.current_set.round_id = 27_140;
    refreshed.current_set.utime_since = 27_140 * 65_536;
    refreshed.current_set.utime_until = 27_141 * 65_536;

    let reason = degraded_refresh_reason(&refreshed, &cached).expect("stale round is degraded");

    assert!(reason.contains("moved backwards"), "unexpected: {reason}");
}

fn strip_active_round_data(snapshot: &mut ClockSnapshot) {
    snapshot.current_set.total_stake = None;
    snapshot.current_set.total_reward = None;
    for validator in &mut snapshot.current_set.validators {
        validator.wallet = None;
        validator.source = None;
        validator.contract_type = None;
        validator.contract_type_hash = None;
        validator.stake = None;
        validator.reward = None;
    }
}

fn test_candidate() -> crate::chain::ElectionCandidateDto {
    crate::chain::ElectionCandidateDto {
        public_key: "candidate-key".to_owned(),
        stake: "100".to_owned(),
        stake_raw: "100".to_owned(),
        created_at: 1_500,
        stake_factor: 1,
        wallet: "-1:candidate".to_owned(),
        source: None,
        contract_type: None,
        contract_type_hash: None,
        adnl_addr: "candidate-adnl".to_owned(),
        history: Vec::new(),
    }
}

struct MockTonCenter {
    timings_boc: String,
    validator_set_boc: String,
    elector_address_boc: String,
    code_boc: String,
    wallet_address: String,
    account_states_requests: AtomicUsize,
}

impl MockTonCenter {
    fn new() -> Result<Self> {
        let code = {
            let mut builder = CellBuilder::new();
            builder.store_u32(0x1234_5678)?;
            builder.build()?
        };

        Ok(Self {
            timings_boc: boc(ElectionTimings {
                validators_elected_for: u32::MAX,
                elections_start_before: 120,
                elections_end_before: 60,
                stake_held_for: 120,
            })?,
            validator_set_boc: boc(ValidatorSet {
                utime_since: 1,
                utime_until: u32::MAX,
                main: NonZeroU16::new(1).unwrap(),
                total_weight: 100,
                list: vec![ValidatorDescription {
                    public_key: HashBytes([0x11; 32]),
                    weight: 100,
                    adnl_addr: Some(HashBytes([0x22; 32])),
                    mc_seqno_since: 0,
                    prev_total_weight: 0,
                }],
            })?,
            elector_address_boc: boc(HashBytes([0x33; 32]))?,
            code_boc: Boc::encode_base64(&code),
            wallet_address: format!("-1:{}", "44".repeat(32)),
            account_states_requests: AtomicUsize::new(0),
        })
    }
}

async fn spawn_mock_toncenter(mock: Arc<MockTonCenter>) -> Result<String> {
    let app = Router::new()
        .route("/api/v2/jsonRPC", post(mock_json_rpc))
        .route("/api/v3/accountStates", get(mock_account_states))
        .with_state(mock);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(format!("http://{addr}"))
}

async fn mock_json_rpc(
    State(mock): State<Arc<MockTonCenter>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "getMasterchainInfo" => json!({
            "last": {
                "seqno": 12345
            }
        }),
        "getConfigParam" => {
            let config_id = params
                .get("config_id")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            match config_id {
                1 => config_response(&mock.elector_address_boc),
                15 => config_response(&mock.timings_boc),
                34 => config_response(&mock.validator_set_boc),
                36 => json!({ "config": null }),
                _ => json!({ "config": null }),
            }
        }
        "runGetMethod" => match params.get("method").and_then(Value::as_str).unwrap_or("") {
            "participant_list_extended" => participant_list_stack(),
            "past_elections" => json!({
                "stack": [
                    {
                        "list": {
                            "elements": []
                        }
                    }
                ],
                "exit_code": 0
            }),
            _ => json!({
                "stack": [],
                "exit_code": 0
            }),
        },
        _ => json!(null),
    };

    Json(json!({
        "ok": true,
        "result": result
    }))
}

async fn mock_account_states(State(mock): State<Arc<MockTonCenter>>) -> Json<Value> {
    mock.account_states_requests.fetch_add(1, Ordering::SeqCst);
    Json(json!({
        "accounts": [
            {
                "address": mock.wallet_address.as_str(),
                "status": "active",
                "code_boc": mock.code_boc.as_str(),
                "data_boc": mock.code_boc.as_str()
            }
        ]
    }))
}

fn config_response(bytes: &str) -> Value {
    json!({
        "config": {
            "bytes": bytes
        }
    })
}

fn participant_list_stack() -> Value {
    json!({
        "stack": [
            number("0"),
            number("0"),
            number("0"),
            number("0"),
            {
                "list": {
                    "elements": [
                        {
                            "tuple": {
                                "elements": [
                                    number(&hex_number(&[0x11; 32])),
                                    {
                                        "tuple": {
                                            "elements": [
                                                number("1000000000"),
                                                number("1"),
                                                number(&hex_number(&[0x44; 32])),
                                                number(&hex_number(&[0x55; 32]))
                                            ]
                                        }
                                    }
                                ]
                            }
                        }
                    ]
                }
            }
        ],
        "exit_code": 0
    })
}

fn number(value: &str) -> Value {
    json!({
        "number": {
            "number": value
        }
    })
}

fn hex_number(bytes: &[u8; 32]) -> String {
    format!(
        "0x{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn boc<T: Store>(value: T) -> Result<String> {
    BocRepr::encode_base64(value).map_err(Into::into)
}

fn test_state_dir() -> Result<PathBuf> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "validatorclock_fallback_test_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[tokio::test]
async fn graphql_primary_fetches_snapshot() -> Result<()> {
    let mock = Arc::new(MockGraphql::new()?);
    let endpoint = spawn_mock_graphql(Arc::clone(&mock)).await?;
    let state_dir = test_state_dir()?;
    let config = Arc::new(graphql_test_config(
        &state_dir,
        endpoint.clone(),
        vec!["http://127.0.0.1:9/jrpc-disabled".to_owned()],
    ));
    let state = Arc::new(AppState::new(config));

    let snapshot = get_chain_snapshot_cached_first(Arc::clone(&state), "everscale", true).await?;

    assert!(snapshot.warning.is_none());
    assert_eq!(
        snapshot.selected_endpoint.as_deref(),
        Some(endpoint.as_str())
    );
    assert_eq!(snapshot.seqno, MockGraphql::SEQ_NO);
    assert_eq!(snapshot.global_id, MockGraphql::GLOBAL_ID);
    assert_eq!(snapshot.params15.validators_elected_for, u32::MAX);
    assert_eq!(snapshot.current_set.total, 1);
    assert_eq!(
        snapshot.current_set.validators[0].public_key,
        "11".repeat(32)
    );
    assert!(snapshot.election.candidates.is_empty());

    Ok(())
}

#[tokio::test]
async fn jrpc_failure_uses_graphql_fallback() -> Result<()> {
    let mock = Arc::new(MockGraphql::new()?);
    let endpoint = spawn_mock_graphql(Arc::clone(&mock)).await?;
    let state_dir = test_state_dir()?;
    let config = Arc::new(graphql_test_config(
        &state_dir,
        "http://127.0.0.1:9/jrpc-disabled".to_owned(),
        vec![endpoint.clone()],
    ));
    let state = Arc::new(AppState::new(config));

    let snapshot = get_chain_snapshot_cached_first(Arc::clone(&state), "everscale", true).await?;

    // The fallback served this - `selected_endpoint` below is the proof. What
    // reaches the reader is a working clock and no word about the machinery:
    // which endpoint answered is the operator's business, and one of them may
    // be an address on the server itself.
    assert_eq!(
        snapshot.warning, None,
        "the reader should not be told that a fallback RPC answered"
    );
    assert_eq!(
        snapshot.selected_endpoint.as_deref(),
        Some(endpoint.as_str())
    );
    assert_eq!(snapshot.current_set.total, 1);

    Ok(())
}

fn graphql_test_config(
    state_dir: &std::path::Path,
    rpc: String,
    rpc_fallbacks: Vec<String>,
) -> AppConfig {
    AppConfig {
        refresh_timeout_seconds: 15,
        ..AppConfig::for_test_in(
            state_dir,
            vec![ChainConfig {
                id: "everscale".to_owned(),
                name: "Everscale".to_owned(),
                rpc,
                rpc_fallbacks,
                color: "#6347F5".to_owned(),
                token_symbol: "EVER".to_owned(),
                rpc_label: None,
            }],
        )
    }
}

struct MockGraphql {
    config_hash: String,
    elector_hash: String,
    config_data_boc: String,
    elector_data_boc: String,
}

impl MockGraphql {
    const SEQ_NO: u32 = 61_203_692;
    const GLOBAL_ID: i32 = 42;

    fn new() -> Result<Self> {
        let mut config = tycho_types::models::BlockchainConfig::new_empty(HashBytes([0x55; 32]));
        config.params.set_raw(
            15,
            CellBuilder::build_from(ElectionTimings {
                validators_elected_for: u32::MAX,
                elections_start_before: 120,
                elections_end_before: 60,
                stake_held_for: 120,
            })?,
        )?;
        config.params.set_raw(
            34,
            CellBuilder::build_from(ValidatorSet {
                utime_since: 1,
                utime_until: u32::MAX,
                main: NonZeroU16::new(1).unwrap(),
                total_weight: 100,
                list: vec![ValidatorDescription {
                    public_key: HashBytes([0x11; 32]),
                    weight: 100,
                    adnl_addr: Some(HashBytes([0x22; 32])),
                    mc_seqno_since: 0,
                    prev_total_weight: 0,
                }],
            })?,
        )?;
        let params_root = config
            .params
            .as_dict()
            .clone()
            .into_root()
            .ok_or_else(|| anyhow!("config params dictionary is empty"))?;

        Ok(Self {
            config_hash: "55".repeat(32),
            elector_hash: "33".repeat(32),
            config_data_boc: Boc::encode_base64(config_account_data(params_root)?),
            elector_data_boc: Boc::encode_base64(empty_elector_data()?),
        })
    }
}

// The config contract keeps the config params dictionary in the first
// reference of its data cell; the rest of the cell is not read here.
fn config_account_data(params_root: tycho_types::cell::Cell) -> Result<tycho_types::cell::Cell> {
    let mut builder = CellBuilder::new();
    builder.store_reference(params_root)?;
    builder.store_u32(1)?;
    Ok(builder.build()?)
}

// Elector data with no current election and no past elections, encoded the way
// the elector contract stores it (ABI 2.1).
fn empty_elector_data() -> Result<tycho_types::cell::Cell> {
    let mut builder = CellBuilder::new();
    builder.store_bit_zero()?;
    builder.store_bit_zero()?;
    builder.store_bit_zero()?;
    builder.store_small_uint(0, 4)?;
    builder.store_u32(0)?;
    builder.store_u256(&HashBytes::ZERO)?;
    Ok(builder.build()?)
}

async fn spawn_mock_graphql(mock: Arc<MockGraphql>) -> Result<String> {
    let app = Router::new()
        .route("/graphql", post(mock_graphql))
        .with_state(mock);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(format!("http://{addr}/graphql"))
}

async fn mock_graphql(
    State(mock): State<Arc<MockGraphql>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let query = request.get("query").and_then(Value::as_str).unwrap_or("");
    let data = if query.contains("blocks(") {
        json!({
            "last": [{ "seq_no": MockGraphql::SEQ_NO, "global_id": MockGraphql::GLOBAL_ID }],
            "key": [{
                "master": {
                    "config": { "p0": mock.config_hash, "p1": mock.elector_hash }
                }
            }]
        })
    } else if query.contains("accounts(") {
        json!({
            "accounts": [
                {
                    "id": format!("-1:{}", mock.config_hash),
                    "acc_type": 1,
                    "data": mock.config_data_boc,
                },
                {
                    "id": format!("-1:{}", mock.elector_hash),
                    "acc_type": 1,
                    "data": mock.elector_data_boc,
                }
            ]
        })
    } else {
        json!({ "transactions": [] })
    };

    Json(json!({ "data": data }))
}

/// The relation that made a request-time refresh pointless: it was given
/// ninety seconds inside a request that lasts ten.
#[test]
fn a_refresh_a_reader_waits_for_fits_inside_the_request_it_waits_in() {
    let configured_longer_than_a_request = foreground_refresh_timeout(90);

    assert!(
        configured_longer_than_a_request < crate::server::connection::REQUEST_TIMEOUT,
        "a refresh that outlasts the request is cancelled at the door with its work thrown away"
    );
    assert_eq!(
        configured_longer_than_a_request,
        crate::server::connection::REQUEST_TIMEOUT - FOREGROUND_REFRESH_MARGIN,
        "and what is left over is for writing the answer out"
    );
    assert_eq!(
        foreground_refresh_timeout(3),
        Duration::from_secs(3),
        "a configured timeout shorter than the request is the one that counts"
    );
}

/// A cold start used to have every reader start a refresh of its own, each one
/// cancelled at ten seconds, none of them recorded.
#[tokio::test]
async fn a_reader_arriving_while_the_chain_refreshes_does_not_start_another() {
    let state = Arc::new(AppState::new(Arc::new(AppConfig::for_test(vec![
        ChainConfig {
            id: "test".to_owned(),
            name: "Test".to_owned(),
            rpc: "http://127.0.0.1:9/nothing-answers-here".to_owned(),
            rpc_fallbacks: Vec::new(),
            color: "#000000".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: None,
        },
    ]))));
    let refreshing = state
        .claim_refresh("test")
        .expect("the first refresh claims the chain");

    let error = get_chain_snapshot_cached_first(Arc::clone(&state), "test", false)
        .await
        .expect_err("nothing is cached and the chain is already being refreshed");

    assert!(
        error.to_string().contains("being refreshed already"),
        "the reader is told why, not left to time out: {error}"
    );

    drop(refreshing);
}
