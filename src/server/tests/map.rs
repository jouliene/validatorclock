use super::*;
use axum::http::{StatusCode, header};
use serde_json::json;
use std::fs;

#[tokio::test]
async fn app_router_serves_bundled_tycho_map_when_no_file_is_configured() {
    let mut config = test_config(Vec::new());
    config.chains.push(test_chain_config(
        "tycho-testnet",
        "Tycho",
        "#58c9f6",
        "TYCHO",
    ));
    let state = state_from_config(config);
    cache_tycho_snapshot(
        &state,
        &["1778eb66b9386bcc37031cad14d73e4554413b23d16b4b680726375a622f3a5b"],
    )
    .await;

    let response = app_response(state, "/api/chains/tycho-testnet/map").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(
        body[0]["peer"],
        "1778eb66b9386bcc37031cad14d73e4554413b23d16b4b680726375a622f3a5b"
    );
    assert!(body[0]["ip"].is_string());
}

#[tokio::test]
async fn app_router_serves_bundled_ton_map_when_no_file_is_configured() {
    let mut config = test_config(Vec::new());
    config
        .chains
        .push(test_chain_config("ton", "TON", "#4DB8FF", "TON"));
    let state = state_from_config(config);
    cache_snapshot(
        &state,
        "ton",
        &["63345c7d7dbcc14f8bce8811cf3fba41981ec0d80d4bfc6c5e089fb82f867a5e"],
    )
    .await;

    let response = app_response(state, "/api/chains/ton/map").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(
        body[0]["peer"],
        "63345c7d7dbcc14f8bce8811cf3fba41981ec0d80d4bfc6c5e089fb82f867a5e"
    );
    assert!(body[0]["ip"].is_string());
}

#[tokio::test]
async fn app_router_serves_configured_tycho_map_file() {
    let map_path = temp_map_path("tycho");
    fs::write(
        &map_path,
        r#"[
            {"peer":"active-validator-public-key","ip":"203.0.113.10","city":"Test City","country":"Testland","isp":"Test ISP","lat":1.25,"lon":2.5},
            {"peer":"inactive-validator-public-key","ip":"203.0.113.11","city":"Other City","country":"Testland","isp":"Test ISP","lat":3.25,"lon":4.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.tycho_map_nodes_path = Some(map_path.clone());
    config.chains.push(test_chain_config(
        "tycho-testnet",
        "Tycho",
        "#58c9f6",
        "TYCHO",
    ));
    let state = state_from_config(config);
    cache_tycho_snapshot(&state, &["active-validator-public-key"]).await;

    let response = app_response(state, "/api/chains/tycho-testnet/map").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["peer"], "active-validator-public-key");
    assert_eq!(body[0]["ip"], "203.0.113.10");

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_serves_configured_ton_map_file() {
    let map_path = temp_map_path("ton");
    fs::write(
        &map_path,
        r#"[
            {"peer":"active-ton-validator","ip":"203.0.113.20","city":"TON City","country":"TONland","isp":"TON ISP","lat":5.25,"lon":6.5},
            {"peer":"inactive-ton-validator","ip":"203.0.113.21","city":"Other City","country":"TONland","isp":"TON ISP","lat":7.25,"lon":8.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config
        .map_nodes_paths
        .insert("ton".to_owned(), map_path.clone());
    config
        .chains
        .push(test_chain_config("ton", "TON", "#4DB8FF", "TON"));
    let state = state_from_config(config);
    cache_snapshot(&state, "ton", &["active-ton-validator"]).await;

    let response = app_response(state, "/api/chains/ton/map").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["peer"], "active-ton-validator");
    assert_eq!(body[0]["ip"], "203.0.113.20");

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_serves_configured_everscale_map_file() {
    let map_path = temp_map_path("everscale");
    fs::write(
        &map_path,
        r#"[
            {"peer":"active-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5},
            {"peer":"inactive-ever-validator","ip":"203.0.113.31","city":"Other City","country":"EVERland","isp":"EVER ISP","lat":11.25,"lon":12.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config
        .map_nodes_paths
        .insert("everscale".to_owned(), map_path.clone());
    config.chains.push(test_chain_config(
        "everscale",
        "Everscale",
        "#6347F5",
        "EVER",
    ));
    let state = state_from_config(config);
    cache_snapshot(&state, "everscale", &["active-ever-validator"]).await;

    let response = app_response(state, "/api/chains/everscale/map").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["peer"], "active-ever-validator");
    assert_eq!(body[0]["ip"], "203.0.113.30");

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_marks_configured_ton_validators_without_map_ip_as_fake() {
    let map_path = temp_map_path("ton_fake");
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ton-validator","ip":"203.0.113.20","city":"TON City","country":"TONland","isp":"TON ISP","lat":5.25,"lon":6.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.history_path = Some(temp_state_path("history_fake_grace"));
    config
        .map_nodes_paths
        .insert("ton".to_owned(), map_path.clone());
    config
        .chains
        .push(test_chain_config("ton", "TON", "#4DB8FF", "TON"));
    let state = state_from_config(config);
    cache_snapshot(
        &state,
        "ton",
        &["mapped-ton-validator", "missing-ton-validator"],
    )
    .await;

    let response = app_response(state, "/api/chains/ton/clock").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body["current_set"]["fake_validator_peers"]
            .as_array()
            .unwrap(),
        &vec![Value::String("missing-ton-validator".to_owned())]
    );
    assert_eq!(
        body["current_set"]["validators"][0]["map_node"],
        json!({
            "ip": "203.0.113.20",
            "isp": "TON ISP",
            "city": "TON City",
            "country": "TONland"
        })
    );

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_clears_stale_map_node_for_fake_validator() {
    let map_path = temp_map_path("everscale_fake_stale_location");
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.history_path = Some(temp_state_path("history_everscale_fake_stale_location"));
    config
        .map_nodes_paths
        .insert("everscale".to_owned(), map_path.clone());
    config.chains.push(test_chain_config(
        "everscale",
        "Everscale",
        "#6347F5",
        "EVER",
    ));
    let state = state_from_config(config);
    cache_snapshot_with(
        &state,
        "everscale",
        &["mapped-ever-validator", "missing-ever-validator"],
        |snapshot| {
            snapshot.current_set.validators[1].map_node = Some(crate::chain::ValidatorMapNodeDto {
                ip: Some("198.51.100.99".to_owned()),
                isp: Some("Old ISP".to_owned()),
                city: Some("Old City".to_owned()),
                country: Some("Oldland".to_owned()),
            });
        },
    )
    .await;

    let response = app_response(state, "/api/chains/everscale/clock").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body["current_set"]["fake_validator_peers"]
            .as_array()
            .unwrap(),
        &vec![Value::String("missing-ever-validator".to_owned())]
    );
    assert_eq!(
        body["current_set"]["validators"][0]["map_node"],
        json!({
            "ip": "203.0.113.30",
            "isp": "EVER ISP",
            "city": "EVER City",
            "country": "EVERland"
        })
    );
    assert!(
        body["current_set"]["validators"][1]
            .get("map_node")
            .is_none(),
        "fake validator unexpectedly kept map_node: {}",
        body["current_set"]["validators"][1]
    );
    assert_eq!(
        body["current_set"]["validators"][1]["last_known_map_node"],
        json!({
            "ip": "198.51.100.99",
            "isp": "Old ISP",
            "city": "Old City",
            "country": "Oldland"
        })
    );
    assert_eq!(
        body["current_set"]["validators"][1]["history"][4]["map_node"],
        json!({
            "ip": "198.51.100.99",
            "isp": "Old ISP",
            "city": "Old City",
            "country": "Oldland"
        })
    );

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_keeps_recently_mapped_everscale_validator_out_of_fake_grace() {
    let map_path = temp_map_path("everscale_fake_retention_grace");
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5},
            {"peer":"grace-ever-validator","ip":"203.0.113.31","city":"Grace City","country":"EVERland","isp":"Grace ISP","lat":11.25,"lon":12.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.history_path = Some(temp_state_path("history_everscale_fake_retention_grace"));
    config
        .map_nodes_paths
        .insert("everscale".to_owned(), map_path.clone());
    config.chains.push(test_chain_config(
        "everscale",
        "Everscale",
        "#6347F5",
        "EVER",
    ));
    let state = state_from_config(config);
    cache_snapshot(
        &state,
        "everscale",
        &[
            "mapped-ever-validator",
            "grace-ever-validator",
            "missing-ever-validator",
        ],
    )
    .await;

    let first =
        response_json(app_response(state.clone(), "/api/chains/everscale/clock").await).await;
    assert_eq!(
        first["current_set"]["fake_validator_peers"]
            .as_array()
            .unwrap(),
        &vec![Value::String("missing-ever-validator".to_owned())]
    );

    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5}
        ]"#,
    )
    .unwrap();

    let second = response_json(app_response(state, "/api/chains/everscale/clock").await).await;
    assert_eq!(
        second["current_set"]["fake_validator_peers"]
            .as_array()
            .unwrap(),
        &vec![Value::String("missing-ever-validator".to_owned())],
        "recently mapped validator was incorrectly marked fake: {}",
        second["current_set"]["fake_validator_peers"]
    );
    assert_eq!(
        second["current_set"]["validators"][1]["map_node"],
        json!({
            "ip": "203.0.113.31",
            "isp": "Grace ISP",
            "city": "Grace City",
            "country": "EVERland"
        }),
        "grace validator should replay its last known map_node without being marked fake"
    );

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_expires_everscale_fake_grace_after_sixty_minutes() {
    let map_path = temp_map_path("everscale_fake_retention_expired");
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5}
        ]"#,
    )
    .unwrap();

    let now = now_sec_for_test();
    let history_base_path = temp_state_path("history_everscale_fake_retention_expired");
    let history_path = crate::history::round_history_chain_path(&history_base_path, "everscale");
    fs::write(
        &history_path,
        serde_json::to_string_pretty(&json!({
            "version": 1,
            "chains": {
                "everscale": {
                    "rounds": {
                        "10": {
                            "round_id": 10,
                            "round_color": "blue",
                            "utime_since": 1000,
                            "utime_until": 2000,
                            "observed_at": now,
                            "validators": {
                                "grace-ever-validator": {
                                    "wallet": "-1:wallet",
                                    "map_node": {
                                        "ip": "203.0.113.31",
                                        "isp": "Grace ISP",
                                        "city": "Grace City",
                                        "country": "EVERland"
                                    },
                                    "map_seen_at": now - 3601,
                                    "fake_node": false
                                }
                            }
                        }
                    }
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.history_path = Some(history_base_path.clone());
    config
        .map_nodes_paths
        .insert("everscale".to_owned(), map_path.clone());
    config.chains.push(test_chain_config(
        "everscale",
        "Everscale",
        "#6347F5",
        "EVER",
    ));
    let state = state_from_config(config);
    cache_snapshot(
        &state,
        "everscale",
        &[
            "mapped-ever-validator",
            "grace-ever-validator",
            "missing-ever-validator",
        ],
    )
    .await;

    let body = response_json(app_response(state, "/api/chains/everscale/clock").await).await;
    assert_eq!(
        body["current_set"]["fake_validator_peers"]
            .as_array()
            .unwrap(),
        &vec![
            Value::String("grace-ever-validator".to_owned()),
            Value::String("missing-ever-validator".to_owned())
        ]
    );

    let _ = fs::remove_file(map_path);
    let _ = fs::remove_file(history_path);
}

#[tokio::test]
async fn app_router_defers_fake_everscale_validators_for_new_set_even_after_map_refresh() {
    let map_path = temp_map_path("everscale_fake_grace");
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.history_path = Some(temp_state_path("history_everscale_fake_grace"));
    config
        .map_nodes_paths
        .insert("everscale".to_owned(), map_path.clone());
    config.chains.push(test_chain_config(
        "everscale",
        "Everscale",
        "#6347F5",
        "EVER",
    ));
    let state = state_from_config(config);
    cache_snapshot_with(
        &state,
        "everscale",
        &["mapped-ever-validator", "missing-ever-validator"],
        |snapshot| {
            snapshot.current_set.utime_since = u32::MAX;
        },
    )
    .await;

    let response = app_response(state, "/api/chains/everscale/clock").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(
        body["current_set"].get("fake_validator_peers").is_none(),
        "unexpected fake peers: {}",
        body["current_set"]["fake_validator_peers"]
    );
    assert_eq!(
        body["current_set"]["validators"][0]["map_node"],
        json!({
            "ip": "203.0.113.30",
            "isp": "EVER ISP",
            "city": "EVER City",
            "country": "EVERland"
        })
    );

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_defers_fake_ton_validators_for_new_set_until_map_refresh() {
    let map_path = temp_map_path("ton_fake_grace");
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ton-validator","ip":"203.0.113.20","city":"TON City","country":"TONland","isp":"TON ISP","lat":5.25,"lon":6.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.history_path = Some(temp_state_path("history_ton_fake_grace"));
    config
        .map_nodes_paths
        .insert("ton".to_owned(), map_path.clone());
    config
        .chains
        .push(test_chain_config("ton", "TON", "#4DB8FF", "TON"));
    let state = state_from_config(config);
    cache_snapshot_with(
        &state,
        "ton",
        &["mapped-ton-validator", "missing-ton-validator"],
        |snapshot| {
            snapshot.current_set.utime_since = u32::MAX;
        },
    )
    .await;

    let response = app_response(state, "/api/chains/ton/clock").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(
        body["current_set"].get("fake_validator_peers").is_none(),
        "unexpected fake peers: {}",
        body["current_set"]["fake_validator_peers"]
    );
    assert_eq!(
        body["current_set"]["validators"][0]["map_node"],
        json!({
            "ip": "203.0.113.20",
            "isp": "TON ISP",
            "city": "TON City",
            "country": "TONland"
        })
    );

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_rejects_map_for_chain_without_map_file() {
    let response = app_response(test_state(Vec::new()), "/api/chains/test/map").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["code"], "map_not_available");
}
