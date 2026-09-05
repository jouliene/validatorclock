use super::*;
use axum::http::{StatusCode, header};
use serde_json::json;
use std::fs;
use std::sync::Arc;

#[tokio::test]
async fn a_chain_with_no_map_file_is_shown_as_having_no_map() {
    // There used to be a snapshot of each map compiled into the binary and
    // served whenever the real one was missing. It hid exactly the failure it
    // was supposed to cushion: with the collector stopped and the file deleted
    // on purpose, the page went on drawing 393 TON nodes from a picture four
    // months old, and nothing anywhere said so.
    for (chain_id, name, colour, symbol, peer) in [
        (
            "tycho-testnet",
            "Tycho",
            "#58c9f6",
            "TYCHO",
            "1778eb66b9386bcc37031cad14d73e4554413b23d16b4b680726375a622f3a5b",
        ),
        (
            "ton",
            "TON",
            "#4DB8FF",
            "TON",
            "63345c7d7dbcc14f8bce8811cf3fba41981ec0d80d4bfc6c5e089fb82f867a5e",
        ),
    ] {
        let mut config = test_config(Vec::new());
        config
            .chains
            .push(test_chain_config(chain_id, name, colour, symbol));
        let state = state_from_config(config);
        if chain_id == "tycho-testnet" {
            cache_tycho_snapshot(&state, &[peer]).await;
        } else {
            cache_snapshot(&state, chain_id, &[peer]).await;
        }

        let response = app_response(state, &format!("/api/chains/{chain_id}/map")).await;

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{chain_id} has no map file, so it has no map"
        );
        let body = response_json(response).await;
        assert_eq!(body["code"], "map_not_available");
    }
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
    config
        .map_nodes_paths
        .insert("tycho-testnet".to_owned(), map_path.clone());
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

/// A validator the map placed two hours ago and has not placed since: called
/// fake, shown without a current position, and remembered where it was.
///
/// Where it was is the history's to say, not the snapshot's. The snapshot
/// carries what the chain said, and the chain says nothing about addresses.
#[tokio::test]
async fn a_validator_the_map_has_stopped_placing_is_fake_and_remembered() {
    let map_path = temp_map_path("everscale_fake_stale_location");
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5},
            {"peer":"missing-ever-validator","ip":"198.51.100.99","city":"Old City","country":"Oldland","isp":"Old ISP","lat":1.25,"lon":2.5}
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
    let two_hours_ago = now_sec_for_test() - 2 * 60 * 60;
    cache_snapshot_seen_at(
        &state,
        "everscale",
        &["mapped-ever-validator", "missing-ever-validator"],
        two_hours_ago,
    )
    .await;

    // The map has been republished without it, and the hour a validator is
    // given after its last sighting has long run out.
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ever-validator","ip":"203.0.113.30","city":"EVER City","country":"EVERland","isp":"EVER ISP","lat":9.25,"lon":10.5}
        ]"#,
    )
    .unwrap();
    let _ = state.refresh_ready_snapshot("everscale").await;

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

    // The map was republished. In the running site the node location pass
    // works the answer out again at that point; nothing else would notice.
    let _ = state.refresh_ready_snapshot("everscale").await;

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
        Value::Null,
        "the current map does not place it, and saying otherwise is how a page \
         reports 393 mapped validators with no map at all"
    );
    assert_eq!(
        second["current_set"]["validators"][1]["last_known_map_node"],
        json!({
            "ip": "203.0.113.31",
            "isp": "Grace ISP",
            "city": "Grace City",
            "country": "EVERland"
        }),
        "where it was last seen is remembered, in the field kept for that"
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

/// The map is written out with the snapshot, so it is served the way the clock
/// is: one tag over bytes that already exist, and nothing sent to a reader that
/// already has them.
#[tokio::test]
async fn the_map_is_served_from_the_copy_written_out_with_the_snapshot() {
    let map_path = temp_map_path("ton_rendered");
    fs::write(
        &map_path,
        r#"[{"peer":"active-ton-validator","ip":"203.0.113.20","city":"TON City","country":"TONland","isp":"TON ISP","lat":5.25,"lon":6.5}]"#,
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

    let response = app_response(Arc::clone(&state), "/api/chains/ton/map").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::ETAG, "W/\"");
    let entity_tag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("a served map carries its tag")
        .to_owned();

    let unchanged = conditional_response(state, "/api/chains/ton/map", &entity_tag).await;

    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);

    let _ = fs::remove_file(map_path);
}

/// One read of the map file a refresh, not one a reader. A file replaced in
/// between is served from the next time the page is worked out - which every
/// refresh does, so a minute at the outside.
#[tokio::test]
async fn a_map_replaced_on_disk_is_served_once_the_page_is_worked_out_again() {
    let map_path = temp_map_path("ton_replaced");
    fs::write(
        &map_path,
        r#"[{"peer":"active-ton-validator","ip":"203.0.113.20","city":"TON City","country":"TONland","isp":"TON ISP","lat":5.25,"lon":6.5}]"#,
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

    let first = response_json(app_response(Arc::clone(&state), "/api/chains/ton/map").await).await;
    assert_eq!(first[0]["ip"], "203.0.113.20");

    fs::write(
        &map_path,
        r#"[{"peer":"active-ton-validator","ip":"198.51.100.77","city":"Moved City","country":"TONland","isp":"Another TON ISP","lat":9.25,"lon":10.5}]"#,
    )
    .unwrap();

    let before_rebuild =
        response_json(app_response(Arc::clone(&state), "/api/chains/ton/map").await).await;
    assert_eq!(
        before_rebuild[0]["ip"], "203.0.113.20",
        "readers are served the copy written out with the snapshot, not the file"
    );

    state.refresh_ready_snapshot("ton").await;

    let after_rebuild = response_json(app_response(state, "/api/chains/ton/map").await).await;
    assert_eq!(after_rebuild[0]["ip"], "198.51.100.77");
    assert_eq!(after_rebuild[0]["city"], "Moved City");

    let _ = fs::remove_file(map_path);
}
