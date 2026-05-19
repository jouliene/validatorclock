use super::util::now_sec;
use super::validator_sources::{
    apply_cached_validator_contract_type_hashes, update_validator_contract_type_hashes,
};
use super::{ClockSnapshot, fetch_chain_snapshot};
use crate::config::ChainConfig;
use crate::state::AppState;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;
use tokio::time::{Duration, MissedTickBehavior, interval, timeout};
use tracing::{info, warn};

const BACKGROUND_REFRESH_CONCURRENCY: usize = 2;
const VALIDATOR_TYPE_UPDATE_MIN_TIMEOUT_SECS: u64 = 5;
const VALIDATOR_TYPE_UPDATE_MAX_TIMEOUT_SECS: u64 = 30;
const STALE_REFRESH_WARNING: &str = "refresh is running in background";

#[derive(Clone, Copy)]
enum RefreshLogKind {
    Background,
    StaleCache,
}

impl RefreshLogKind {
    fn label(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::StaleCache => "stale_cache",
        }
    }
}

pub(crate) fn spawn_background_refresh(state: Arc<AppState>) {
    tokio::spawn(async move {
        background_refresh_loop(state).await;
    });
}

async fn background_refresh_loop(state: Arc<AppState>) {
    let refresh_seconds = state.config.refresh_seconds.max(10);
    info!(refresh_seconds, "background chain refresh started");
    let mut ticker = interval(Duration::from_secs(refresh_seconds));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        refresh_configured_chains(Arc::clone(&state)).await;
    }
}

async fn refresh_configured_chains(state: Arc<AppState>) {
    let mut chain_ids = state
        .config
        .chains
        .iter()
        .map(|chain| chain.id.clone())
        .collect::<Vec<_>>()
        .into_iter();
    let mut tasks = JoinSet::new();

    loop {
        while tasks.len() < BACKGROUND_REFRESH_CONCURRENCY {
            let Some(chain_id) = chain_ids.next() else {
                break;
            };
            let task_state = Arc::clone(&state);
            tasks.spawn(async move {
                refresh_chain_and_log(&task_state, &chain_id, RefreshLogKind::Background).await;
            });
        }

        if tasks.is_empty() {
            break;
        }

        if let Some(result) = tasks.join_next().await
            && let Err(error) = result
        {
            warn!(
                error = ?error,
                "background refresh task failed"
            );
        }
    }
}

async fn refresh_chain_and_log(state: &AppState, chain_id: &str, log_kind: RefreshLogKind) {
    let refresh_kind = log_kind.label();
    let started_at = Instant::now();
    match get_chain_snapshot(state, chain_id, true).await {
        Ok(snapshot) if snapshot.warning.is_some() => {
            info!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                fetched_at = snapshot.fetched_at,
                round_id = snapshot.current_set.round_id,
                round_color = ?snapshot.current_set.round_color,
                warning = ?snapshot.warning,
                "chain refresh completed with cached data"
            );
        }
        Ok(snapshot) => {
            info!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                fetched_at = snapshot.fetched_at,
                round_id = snapshot.current_set.round_id,
                round_color = ?snapshot.current_set.round_color,
                "chain refresh completed"
            );
        }
        Err(error) => {
            warn!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                error = ?error,
                "chain refresh failed"
            );
        }
    }
}

async fn get_chain_snapshot(
    state: &AppState,
    chain_id: &str,
    force_refresh: bool,
) -> Result<ClockSnapshot> {
    let now = now_sec()?;
    let refresh_seconds = state.config.refresh_seconds.max(10);

    if !force_refresh
        && let Some(snapshot) = state
            .cached_snapshot_if_fresh(chain_id, now, refresh_seconds)
            .await
    {
        return Ok(snapshot);
    }

    let chain = state
        .config
        .chain(chain_id)
        .ok_or_else(|| anyhow!("unknown chain id `{chain_id}`"))?;
    state.record_refresh_attempt(chain_id, now).await;

    let timeout_seconds = state.config.refresh_timeout_seconds;
    let refresh_result = timeout(
        Duration::from_secs(timeout_seconds),
        fetch_chain_snapshot_with_validator_types(state, chain),
    )
    .await
    .unwrap_or_else(|_| Err(anyhow!("refresh timed out after {timeout_seconds}s")));

    match refresh_result {
        Ok(mut snapshot) => {
            let fetched_at = snapshot.fetched_at;
            let observed_at = now_sec().unwrap_or(snapshot.fetched_at);
            state.annotate_tycho_fake_validators(&mut snapshot);
            state.record_round_history(&mut snapshot, observed_at).await;
            apply_cached_validator_contract_type_hashes(state, chain, &mut snapshot).await;
            state
                .store_cached_snapshot(chain_id, now, snapshot.clone())
                .await;
            state.record_refresh_success(chain_id, fetched_at).await;
            Ok(snapshot)
        }
        Err(error) => {
            let error_message = error.to_string();
            state
                .record_refresh_failure(chain_id, now, error_message)
                .await;
            if let Some(mut snapshot) = state.cached_snapshot(chain_id).await {
                snapshot.warning = Some(format!(
                    "using cached data from {}; refresh failed: {error}",
                    snapshot.fetched_at
                ));
                return Ok(snapshot);
            }
            Err(error)
        }
    }
}

pub(crate) async fn get_chain_snapshot_cached_first(
    state: Arc<AppState>,
    chain_id: &str,
    force_refresh: bool,
) -> Result<ClockSnapshot> {
    let now = now_sec()?;
    let refresh_seconds = state.config.refresh_seconds.max(10);

    if !force_refresh {
        if let Some(snapshot) = state
            .cached_snapshot_if_fresh(chain_id, now, refresh_seconds)
            .await
        {
            return Ok(snapshot);
        }

        if let Some(mut snapshot) = state.cached_snapshot(chain_id).await {
            snapshot.warning = Some(format!(
                "using cached data from {}; {STALE_REFRESH_WARNING}",
                snapshot.fetched_at
            ));
            spawn_stale_snapshot_refresh(Arc::clone(&state), chain_id.to_owned(), now).await;
            return Ok(snapshot);
        }
    }

    get_chain_snapshot(&state, chain_id, force_refresh).await
}

async fn spawn_stale_snapshot_refresh(state: Arc<AppState>, chain_id: String, now: u64) {
    let retry_after_seconds = state
        .config
        .refresh_seconds
        .max(10)
        .min(state.config.refresh_timeout_seconds.max(10));
    if !state
        .mark_refresh_attempt_if_due(&chain_id, now, retry_after_seconds)
        .await
    {
        return;
    }

    tokio::spawn(async move {
        refresh_chain_and_log(&state, &chain_id, RefreshLogKind::StaleCache).await;
    });
}

async fn fetch_chain_snapshot_with_validator_types(
    state: &AppState,
    chain: &ChainConfig,
) -> Result<ClockSnapshot> {
    let mut snapshot = fetch_chain_snapshot(chain).await?;
    let type_update_timeout =
        Duration::from_secs((state.config.refresh_timeout_seconds / 3).clamp(
            VALIDATOR_TYPE_UPDATE_MIN_TIMEOUT_SECS,
            VALIDATOR_TYPE_UPDATE_MAX_TIMEOUT_SECS,
        ));
    match timeout(
        type_update_timeout,
        update_validator_contract_type_hashes(state, chain, &mut snapshot),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            warn!(
                chain_id = %chain.id,
                error = ?error,
                "failed to update validator contract type hashes"
            );
        }
        Err(_) => {
            warn!(
                chain_id = %chain.id,
                timeout_seconds = type_update_timeout.as_secs(),
                "validator contract type hash update timed out"
            );
        }
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, SecurityConfig, TlsConfig};
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
            listen: "127.0.0.1:0".to_owned(),
            refresh_seconds: 60,
            refresh_timeout_seconds: 15,
            cache_path: state_dir.join("cache.json"),
            history_path: Some(state_dir.join("history.json")),
            tycho_map_nodes_path: None,
            security: SecurityConfig::default(),
            tls: TlsConfig::default(),
            chains: vec![ChainConfig {
                id: "ton".to_owned(),
                name: "TON".to_owned(),
                rpc: "http://127.0.0.1:9/broxus-disabled".to_owned(),
                rpc_fallbacks: vec![format!("{endpoint}/api/v2/jsonRPC")],
                color: "#0098ea".to_owned(),
                token_symbol: "TON".to_owned(),
                rpc_label: None,
            }],
        });
        let state = Arc::new(AppState::new(config));

        let snapshot = get_chain_snapshot_cached_first(Arc::clone(&state), "ton", true).await?;

        assert!(
            snapshot
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("using TON Center fallback"))
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
            "validators_clock_fallback_test_{}_{}",
            std::process::id(),
            nonce
        ));
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}
