use super::*;
use crate::config::{AppConfig, SecurityConfig, TlsConfig};
use axum::Router;
use axum::extract::{Json, State};
use axum::routing::{get, post};
use minik2::{HashBytes, ValidatorSet};
use serde_json::{Value, json};
use std::collections::HashMap;
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
        map_nodes_paths: HashMap::new(),
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
