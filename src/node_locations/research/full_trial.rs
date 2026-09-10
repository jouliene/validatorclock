//! Explicit local all-chain comparison. Does not run with ordinary tests.
use super::super::{
    candidates::collect_candidates_from_value, geo_cache, manual_review::ManualResolvedIp,
};
use super::*;
use serde_json::json;
use std::collections::BTreeSet;

#[tokio::test]
#[ignore = "explicit all-network comparison with real free-service queries"]
async fn full_network_comparison() {
    crate::tls::install_rustls_crypto_provider();
    let root = PathBuf::from(
        std::env::var("VALIDATORCLOCK_FULL_GEO_DIR").expect("set snapshot directory"),
    );
    let snapshot: Value =
        serde_json::from_slice(&std::fs::read(root.join("snapshot.json")).unwrap()).unwrap();
    let assets = PathBuf::from(
        std::env::var("VALIDATORCLOCK_GEO_ASSETS_DIR").expect("set existing asset directory"),
    );
    let config = NodeLocationsConfig {
        geo_cache_path: root.join("run/cache.json"),
        ..NodeLocationsConfig::default()
    };
    let mut engine = Engine::open(&config).unwrap();
    let now = crate::timeutil::now_sec();
    if !engine.database_path().exists() {
        std::fs::copy(assets.join("dbip-city-lite.mmdb"), engine.database_path()).unwrap();
        let month = crate::timeutil::day_string(crate::timeutil::day_index(now))[..7].to_owned();
        engine.store.assets.insert(format!("dbip-{month}"), now);
        for (name, file) in [
            ("hetzner", "hetzner-geofeed.txt"),
            ("latitude", "latitude-geofeed.txt"),
        ] {
            std::fs::copy(
                assets.join(file),
                engine.directory.join(format!("{name}.csv")),
            )
            .unwrap();
            engine.store.assets.insert(name.into(), now + 30 * DAY);
        }
        engine.save().unwrap();
    }
    let baseline: geo_cache::GeoCache =
        serde_json::from_value(snapshot["geo_cache"].clone()).unwrap();
    let mut cache = if config.geo_cache_path.exists() {
        geo_cache::load_geo_cache(&config.geo_cache_path).unwrap()
    } else {
        baseline.clone()
    };
    let mut all = BTreeSet::new();
    let mut manual = BTreeMap::new();
    let mut chain_ips = BTreeMap::new();
    let mut baseline_nodes = BTreeMap::new();
    for (chain, data) in snapshot["chains"].as_object().unwrap() {
        let candidates = collect_candidates_from_value(&data["input_path"]["data"], None);
        let mut ips = candidates.iter().map(|c| c.ip).collect::<BTreeSet<_>>();
        for n in data["output_path"]["data"].as_array().unwrap() {
            let ip: IpAddr = n["ip"].as_str().unwrap().parse().unwrap();
            ips.insert(ip);
            baseline_nodes.insert((chain.clone(), ip), n.clone());
            if n["geo_source"] == "manual" {
                manual.insert(ip,serde_json::from_value::<ManualResolvedIp>(json!({"ip":ip.to_string(),"geo":{"city":n["city"],"country":n["country"],"latitude":n["lat"],"longitude":n["lon"]}})).unwrap());
            }
        }
        all.extend(ips.iter().copied());
        chain_ips.insert(chain.clone(), ips);
    }
    let ips = all.iter().copied().collect::<Vec<_>>();
    println!(
        "INPUT unique_ips={} chains={} manual={}",
        ips.len(),
        chain_ips.len(),
        manual.len()
    );
    let normal_scheduler = std::env::var_os("VALIDATORCLOCK_NORMAL_SCHEDULER").is_some();
    let starting_budget = engine.store.budget.clone();
    let run_started_at = crate::timeutil::now_sec();
    let mut cycles = Vec::new();
    if normal_scheduler {
        for cycle in 0..12 {
            for cips in chain_ips.values() {
                engine
                    .refresh(
                        &config,
                        &cips.iter().copied().collect::<Vec<_>>(),
                        &manual,
                        &mut cache,
                        crate::timeutil::now_sec(),
                    )
                    .await
                    .unwrap();
            }
            geo_cache::save_geo_cache(&config.geo_cache_path, &cache).unwrap();
            let pending = ips
                .iter()
                .filter(|ip| !manual.contains_key(ip))
                .filter(|ip| {
                    engine.store.entries.get(&ip.to_string()).is_none_or(|e| {
                        e.proposed_location.is_none()
                            || e.globalping.as_ref().is_some_and(|j| !j.finished)
                            || (!e.followup_locations.is_empty() && e.globalping.is_none())
                            || (e.completed_at == 0
                                && (!e.observations.contains_key("ip-api")
                                    || (!e.reasons.is_empty() && !e.secondary_checked)))
                    })
                })
                .count();
            let item = json!({"cycle":cycle,"at":crate::timeutil::now_sec(),"pending":pending,"requests_since_start":engine.store.budget.total_requests-starting_budget.total_requests});
            println!("NORMAL {}", item);
            cycles.push(item);
            if pending == 0 {
                break;
            }
            // Exact production interval. No per-IP deadlines, timestamps or quotas changed.
            tokio::time::sleep(Duration::from_secs(config.refresh_seconds)).await;
        }
    } else {
        // Historical accelerated harness retained only to reproduce the earlier audit.
        for chunk in ips.chunks(100) {
            engine
                .refresh(
                    &config,
                    chunk,
                    &manual,
                    &mut cache,
                    crate::timeutil::now_sec(),
                )
                .await
                .unwrap();
            geo_cache::save_geo_cache(&config.geo_cache_path, &cache).unwrap();
            println!(
                "INITIAL entries={} requests={} sources={:?}",
                engine.store.entries.len(),
                engine.store.budget.total_requests,
                engine.store.budget.requests_by_source
            );
            tokio::time::sleep(Duration::from_secs(6)).await;
        }
    }
    let initial_requests = engine.store.budget.total_requests;
    let before = initial_requests;
    engine
        .refresh(
            &config,
            &ips,
            &manual,
            &mut cache,
            crate::timeutil::now_sec(),
        )
        .await
        .unwrap();
    let warm_requests = engine.store.budget.total_requests - before;
    drop(engine);
    let mut engine = Engine::open(&config).unwrap();
    let before = engine.store.budget.total_requests;
    engine
        .refresh(
            &config,
            &ips,
            &manual,
            &mut cache,
            crate::timeutil::now_sec(),
        )
        .await
        .unwrap();
    let restart_requests = engine.store.budget.total_requests - before;

    // Explicit audit acceleration: shorten only local IP retry deadlines to drain the queue.
    // NEVER change source not_before, remote quotas, observation times or finished jobs.
    // At most one additional failed lookup per source/IP in this drain; failures are reported.
    let mut failed_primary = BTreeSet::new();
    let mut failed_secondary = BTreeSet::new();
    let mut accelerated = 0;
    for round in 0..if normal_scheduler { 0 } else { 8 } {
        let mut worked = 0;
        for ip in &ips {
            let Some(e) = engine.store.entries.get(&ip.to_string()) else {
                continue;
            };
            if e.completed_at > 0 || manual.contains_key(ip) {
                continue;
            }
            let need_primary =
                !e.observations.contains_key("ip-api") && !failed_primary.contains(ip);
            let need_secondary =
                !e.secondary_checked && !e.reasons.is_empty() && !failed_secondary.contains(ip);
            let need_measure = e.globalping.as_ref().is_some_and(|j| !j.finished)
                || (e.globalping.is_none() && !e.reasons.is_empty());
            if !need_primary && !need_secondary && !need_measure {
                continue;
            }
            let old = engine.store.budget.requests_by_source.clone();
            engine
                .store
                .entries
                .get_mut(&ip.to_string())
                .unwrap()
                .next_attempt_at = crate::timeutil::now_sec();
            accelerated += 1;
            engine
                .refresh(
                    &config,
                    &[*ip],
                    &manual,
                    &mut cache,
                    crate::timeutil::now_sec(),
                )
                .await
                .unwrap();
            let e = &engine.store.entries[&ip.to_string()];
            let sent = |source: &str| {
                engine
                    .store
                    .budget
                    .requests_by_source
                    .get(source)
                    .unwrap_or(&0)
                    > old.get(source).unwrap_or(&0)
            };
            if sent("ip-api") && !e.observations.contains_key("ip-api") {
                failed_primary.insert(*ip);
            }
            if sent("ipwho.is") && !e.secondary_checked {
                failed_secondary.insert(*ip);
            }
            worked += 1;
            geo_cache::save_geo_cache(&config.geo_cache_path, &cache).unwrap();
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }
        println!(
            "DRAIN round={} inspected={} requests={} complete={} disputed={} jobs_finished={}",
            round,
            worked,
            engine.store.budget.total_requests,
            engine
                .store
                .entries
                .values()
                .filter(|e| e.completed_at > 0)
                .count(),
            engine
                .store
                .entries
                .values()
                .filter(|e| e.confidence == "disputed")
                .count(),
            engine
                .store
                .entries
                .values()
                .filter(|e| e.globalping.as_ref().is_some_and(|j| j.finished))
                .count()
        );
        if worked == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_secs(6)).await;
    }
    let mut rows = vec![];
    let mut summaries = BTreeMap::new();
    for (chain, cips) in &chain_ips {
        let mut changed = 0;
        let mut disputed = 0;
        let mut measured = 0;
        let mut complete = 0;
        let mut manual_count = 0;
        for ip in cips {
            let old = baseline_nodes.get(&(chain.clone(), *ip));
            let entry = engine.store.entries.get(&ip.to_string());
            let selected = cache.location(*ip);
            let is_manual = manual.contains_key(ip);
            let km = if is_manual {
                Some(0.0)
            } else {
                old.zip(selected).and_then(|(o, n)| {
                    Some(model::distance(
                        &Observation {
                            lat: o["lat"].as_f64()?,
                            lon: o["lon"].as_f64()?,
                            ..Observation::default()
                        },
                        &Observation::from_cached(n),
                    ))
                })
            };
            if km.is_some_and(|d| d > 100.0) {
                changed += 1;
            }
            if entry.is_some_and(|e| e.confidence == "disputed") {
                disputed += 1;
            }
            if entry.is_some_and(|e| e.confidence == "measured_metro") {
                measured += 1;
            }
            if entry.is_some_and(|e| e.completed_at > 0) {
                complete += 1;
            }
            if is_manual {
                manual_count += 1;
            }
            rows.push(json!({"chain":chain,"ip":ip.to_string(),"manual":is_manual,"old_map":old,"new_cache":selected,"distance_km":km,"entry":entry}));
        }
        let input = &snapshot["chains"][chain]["input_path"]["data"];
        summaries.insert(chain.clone(),json!({"validators_total":input["validators_total"],"resolved_total":input["resolved_total"],"remembered_total":input["remembered_total"],"map_rows":snapshot["chains"][chain]["output_path"]["data"].as_array().unwrap().len(),"unique_ips":cips.len(),"moved_over_100km":changed,"disputed":disputed,"measured_metros":measured,"completed":complete,"manual":manual_count}));
    }
    let mut report = json!({"snapshot_at":snapshot["captured_at"],"finished_at":crate::timeutil::now_sec(),"unique_ips":ips.len(),"chains":summaries,"initial_http_requests":initial_requests,"immediate_warm_http_requests":warm_requests,"immediate_restart_http_requests":restart_requests,"total_budget":engine.store.budget,"accelerated_local_retries":accelerated,"failed_primary":failed_primary,"failed_secondary":failed_secondary,"method":"Production map baseline versus actual Engine on copied node data. Shared official assets preseeded, excluded from HTTP totals. Queue drain shortens only per-IP backoff deadlines, NOT source throttles or timestamps. Not a real-time 48-hour soak. Failed and disputed results are retained, not counted as fixes.","rows":rows});
    report["normal_scheduler"] = json!(normal_scheduler);
    report["starting_budget"] = json!(starting_budget);
    report["run_started_at"] = json!(run_started_at);
    report["cycles"] = json!(cycles);
    if normal_scheduler {
        report["method"] = json!(
            "Actual Engine, all production chains, configured due-queue size and real 300-second worker intervals. No retry deadline overrides. Existing valid observations and completed remote measurements reused across policy migration; previous displayed points from fresh production cache. Shared files reused. This is a real scheduler run, not a real-time 48-hour soak."
        );
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "FINAL {}",
        json!({"chains":report["chains"],"budget":report["total_budget"],"warm":warm_requests,"restart":restart_requests})
    );
}

/// Same primary answers, actual old algorithm, local replay only. No external API traffic.
#[tokio::test]
#[ignore = "requires completed all-network trial recordings"]
async fn full_network_legacy_replay() {
    crate::tls::install_rustls_crypto_provider();
    let root = PathBuf::from(
        std::env::var("VALIDATORCLOCK_FULL_GEO_DIR").expect("set snapshot directory"),
    );
    let snapshot: Value =
        serde_json::from_slice(&std::fs::read(root.join("snapshot.json")).unwrap()).unwrap();
    let report: Value =
        serde_json::from_slice(&std::fs::read(root.join("report.json")).unwrap()).unwrap();
    let mut original: geo_cache::GeoCache =
        serde_json::from_value(snapshot["geo_cache"].clone()).unwrap();
    let entries: BTreeMap<String, Value> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["ip"].as_str().unwrap().to_string(), r["entry"].clone()))
        .collect();
    let replies = std::sync::Arc::new(entries.clone());
    let who = replies.clone();
    let app=axum::Router::new().route("/batch",axum::routing::post(move |axum::Json(ips):axum::Json<Vec<String>>| {let replies=replies.clone();async move {
        axum::Json(ips.iter().map(|ip| {
            let o=&replies[ip]["observations"]["ip-api"];
            if o.is_null() {return json!({"query":ip,"status":"fail"});}
            json!({"query":ip,"status":"success","city":o["city"],"country":o["country"],"countryCode":o["country_code"],"lat":o["lat"],"lon":o["lon"],"isp":o["isp"],"as":o["asn"]})
        }).collect::<Vec<_>>())
    }})).route("/who/{ip}",axum::routing::get(move |axum::extract::Path(ip):axum::extract::Path<String>| {let who=who.clone();async move {
        let o=&who[&ip]["observations"]["ipwho.is"];
        axum::Json(if o.is_null() {json!({"success":false})} else {json!({"ip":ip,"success":true,"city":o["city"],"country":o["country"],"country_code":o["country_code"],"latitude":o["lat"],"longitude":o["lon"],"connection":{"isp":o["isp"],"asn":o["asn"]}})})
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let cfg = NodeLocationsConfig {
        ip_api_batch_endpoint: format!("http://{addr}/batch"),
        tiebreak_base_url: format!("http://{addr}/who"),
        ipinfo_lite_base_url: format!("http://{addr}/unavailable-lite"),
        ipinfo_token: None,
        ipinfo_token_env: "VALIDATORCLOCK_REPLAY_NO_TOKEN".into(),
        ..NodeLocationsConfig::default()
    };
    let mut manual = BTreeMap::new();
    let mut ips = BTreeSet::new();
    for r in report["rows"].as_array().unwrap() {
        let ip: IpAddr = r["ip"].as_str().unwrap().parse().unwrap();
        ips.insert(ip);
        if r["manual"] == true {
            let n = &r["old_map"];
            manual.insert(ip,serde_json::from_value::<ManualResolvedIp>(json!({"ip":ip.to_string(),"geo":{"city":n["city"],"country":n["country"],"latitude":n["lat"],"longitude":n["lon"]}})).unwrap());
        }
    }
    // Force primary refresh only; retain authentic IPinfo/tiebreak evidence and its age.
    for ip in &ips {
        if let Some(p) = original.location_mut(*ip) {
            p.updated_at = 0;
        }
    }
    let now = report["finished_at"].as_u64().unwrap();
    super::super::legacy_refresh(
        crate::http::shared_client(),
        &cfg,
        &ips.iter().copied().collect::<Vec<_>>(),
        &manual,
        &mut original,
        now,
        Duration::from_secs(7 * DAY),
    )
    .await;
    server.abort();
    let rows=report["rows"].as_array().unwrap().iter().map(|r| {
        let ip:IpAddr=r["ip"].as_str().unwrap().parse().unwrap();
        let old=original.location(ip);
        let new=r["new_cache"].clone();
        let km=old.and_then(|o|serde_json::from_value::<geo_cache::CachedGeoLocation>(new.clone()).ok().map(|n|model::distance(&Observation::from_cached(o),&Observation::from_cached(&n))));
        json!({"chain":r["chain"],"ip":r["ip"],"manual":r["manual"],"legacy_same_primary":old,"new_cache":new,"distance_km":km})
    }).collect::<Vec<_>>();
    let result = json!({"method":"Actual legacy_refresh against LOCAL replay of experiment primary answers; original cached IPinfo/tiebreak evidence retained with original ages. Primary TTL intentionally bypassed. No new IPinfo answers, unavailable third answers fail. Counterfactual refresh comparison, not a fresh independent provider census; zero external requests.","rows":rows});
    std::fs::write(
        root.join("legacy-replay.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
#[ignore = "requires completed all-network trial recordings"]
async fn full_network_warm_verification() {
    let root = PathBuf::from(
        std::env::var("VALIDATORCLOCK_FULL_GEO_DIR").expect("set snapshot directory"),
    );
    let mut report: Value =
        serde_json::from_slice(&std::fs::read(root.join("report.json")).unwrap()).unwrap();
    let cfg = NodeLocationsConfig {
        geo_cache_path: root.join("run/cache.json"),
        ..NodeLocationsConfig::default()
    };
    let mut cache = geo_cache::load_geo_cache(&cfg.geo_cache_path).unwrap();
    let mut ips = BTreeSet::new();
    let mut manual = BTreeMap::new();
    for r in report["rows"].as_array().unwrap() {
        let ip: IpAddr = r["ip"].as_str().unwrap().parse().unwrap();
        ips.insert(ip);
        if r["manual"] == true {
            let n = &r["old_map"];
            manual.insert(ip,serde_json::from_value::<ManualResolvedIp>(json!({"ip":ip.to_string(),"geo":{"city":n["city"],"country":n["country"],"latitude":n["lat"],"longitude":n["lon"]}})).unwrap());
        }
    }
    let mut deltas = vec![];
    for _ in 0..2 {
        let mut engine = Engine::open(&cfg).unwrap();
        let before = engine.store.budget.total_requests;
        engine
            .refresh(
                &cfg,
                &ips.iter().copied().collect::<Vec<_>>(),
                &manual,
                &mut cache,
                crate::timeutil::now_sec(),
            )
            .await
            .unwrap();
        deltas.push(engine.store.budget.total_requests - before);
    }
    assert_eq!(deltas, vec![0, 0]);
    report["post_completion_reopen_http_requests"] = json!(deltas);
    report["post_completion_checked_at"] = json!(crate::timeutil::now_sec());
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

/// Verify the 48-hour policy on the complete IP set with a local replay server.
/// This tests scheduling, not accuracy: recorded points are treated as successful inputs.
#[tokio::test]
#[ignore = "requires all-network snapshot; local HTTP replay only"]
async fn full_network_two_day_schedule_replay() {
    use std::sync::{Arc, Mutex};
    let root = PathBuf::from(
        std::env::var("VALIDATORCLOCK_FULL_GEO_DIR").expect("set snapshot directory"),
    );
    let snapshot: Value =
        serde_json::from_slice(&std::fs::read(root.join("snapshot.json")).unwrap()).unwrap();
    let mut ips = BTreeSet::new();
    for chain in snapshot["chains"].as_object().unwrap().values() {
        for node in chain["output_path"]["data"].as_array().unwrap() {
            if node["geo_source"] != "manual" {
                ips.insert(node["ip"].as_str().unwrap().parse::<IpAddr>().unwrap());
            }
        }
    }
    let source: geo_cache::GeoCache =
        serde_json::from_value(snapshot["geo_cache"].clone()).unwrap();
    let source = Arc::new(source);
    let responses = source.clone();
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let counted = calls.clone();
    let app=axum::Router::new().route("/batch",axum::routing::post(move |axum::Json(queries):axum::Json<Vec<String>>|{let responses=responses.clone();let counted=counted.clone();async move {
        counted.lock().unwrap().extend(queries.clone());
        axum::Json(queries.iter().map(|ip| {let p=&responses.locations[ip];json!({"query":ip,"status":"success","city":p.city,"country":p.country,"countryCode":p.country_code,"lat":p.lat,"lon":p.lon,"isp":p.isp,"as":p.asn})}).collect::<Vec<_>>())
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let dir = root.join(format!(
        "schedule-replay-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let config = NodeLocationsConfig {
        geo_cache_path: dir.join("cache.json"),
        ip_api_batch_endpoint: format!("http://{addr}/batch"),
        tiebreak_base_url: format!("http://{addr}/unused"),
        research: ResearchConfig {
            download_database: false,
            network_measurements: false,
            ..ResearchConfig::default()
        },
        ..NodeLocationsConfig::default()
    };
    let now = 2_000_000_000;
    let mut cache = (*source).clone();
    for p in cache.locations.values_mut() {
        p.source = "ip-api".into();
        p.updated_at = now;
    }
    let ips = ips.into_iter().collect::<Vec<_>>();
    let mut engine = Engine::open(&config).unwrap();
    // No operator downloads in this isolated scheduler replay; source ASNs remain realistic.
    engine.store.assets.insert("hetzner".into(), now + 10 * DAY);
    engine
        .store
        .assets
        .insert("latitude".into(), now + 10 * DAY);
    for chunk in ips.chunks(100) {
        engine
            .refresh(&config, chunk, &BTreeMap::new(), &mut cache, now)
            .await
            .unwrap();
    }
    assert_eq!(
        engine
            .store
            .entries
            .values()
            .filter(|e| e.completed_at > 0)
            .count(),
        ips.len()
    );
    for tick in 1..576 {
        engine
            .refresh(
                &config,
                &ips,
                &BTreeMap::new(),
                &mut cache,
                now + tick * 300,
            )
            .await
            .unwrap();
        if tick == 288 {
            engine.save().unwrap();
            drop(engine);
            engine = Engine::open(&config).unwrap();
        }
    }
    assert!(calls.lock().unwrap().is_empty());
    for (i, chunk) in ips.chunks(100).enumerate() {
        engine
            .refresh(
                &config,
                chunk,
                &BTreeMap::new(),
                &mut cache,
                now + 2 * DAY + i as u64 * 10,
            )
            .await
            .unwrap();
    }
    let counts = calls
        .lock()
        .unwrap()
        .iter()
        .fold(BTreeMap::new(), |mut m, ip| {
            *m.entry(ip.clone()).or_insert(0) += 1;
            m
        });
    assert_eq!(counts.len(), ips.len());
    assert!(counts.values().all(|n| *n == 1));
    engine.save().unwrap();
    drop(engine);
    let mut engine = Engine::open(&config).unwrap();
    engine
        .refresh(
            &config,
            &ips,
            &BTreeMap::new(),
            &mut cache,
            now + 2 * DAY + 300,
        )
        .await
        .unwrap();
    assert_eq!(calls.lock().unwrap().len(), ips.len());
    assert!(
        engine
            .store
            .budget
            .requests_by_source
            .keys()
            .all(|s| s == "ip-api")
    );
    let result = json!({"ips":ips.len(),"cycles_before_expiry":575,"restart_at_hours":24,"external_http_requests":0,"per_ip_queries_before_48_hours":0,"per_ip_queries_after_48_hours":1,"primary_batch_calls_at_expiry":engine.store.budget.requests_by_source.get("ip-api"),"all_calls":engine.store.budget.requests_by_source,"non_completed":engine.store.entries.iter().filter(|(_,e)|e.completed_at==0).map(|(ip,e)|json!({"ip":ip,"reasons":e.reasons,"previous":e.previous_location,"proposed":e.proposed_location})).collect::<Vec<_>>(),"note":"Actual Engine against local recorded successful primary points; DB downloads and measurements disabled for isolated scheduler verification. Simulated time, not a 48-hour real-world soak."});
    std::fs::write(
        root.join("two-day-replay.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!("{}", result);
    server.abort();
    std::fs::remove_dir_all(&dir).unwrap();
}
