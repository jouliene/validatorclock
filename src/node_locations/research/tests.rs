use super::model::{Budget, Measurement};
use super::*;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn point(city: &str, code: &str, lat: f64, lon: f64) -> Observation {
    Observation {
        source: "ip-api".into(),
        at: 2_000_000_000,
        city: city.into(),
        country: code.into(),
        country_code: code.into(),
        lat,
        lon,
        asn: Some("AS64500".into()),
        isp: "fixture".into(),
        ..Observation::default()
    }
}
#[test]
fn agreement_on_country_is_not_agreement_on_city() {
    let mut e = Entry::default();
    e.observations
        .insert("ip-api".into(), point("New York", "US", 40.7128, -74.0060));
    let mut alternative = point("Los Angeles", "US", 34.0522, -118.2437);
    alternative.source = "dbip".into();
    e.observations.insert("dbip".into(), alternative);
    decide(&mut e);
    assert_eq!(e.confidence, "disputed");
    assert_eq!(e.decision.unwrap().city, "New York"); // No unproven auto-correction.
}
#[test]
fn majority_does_not_overwrite_primary_country() {
    let mut e = Entry::default();
    e.observations
        .insert("ip-api".into(), point("Amsterdam", "NL", 52.36, 4.9));
    e.observations
        .insert("dbip".into(), point("George Town", "KY", 19.28, -81.37));
    e.observations
        .insert("ipwho.is".into(), point("George Town", "KY", 19.28, -81.37));
    decide(&mut e);
    assert_eq!(e.decision.as_ref().unwrap().country_code, "NL");
    assert_eq!(e.observations.len(), 3);
    assert_eq!(e.confidence, "disputed");
}
#[test]
fn low_rtt_requires_direct_target_multiple_replies_and_uncached_response() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../docs/geolocation-audit-2026-09-10/observations.json"
    ))
    .unwrap();
    let record = data["looking_glass"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["request"]["query_location"] == "sydney")
        .unwrap();
    let mut raw: operators::PingResponse =
        serde_json::from_value(record["response"].clone()).unwrap();
    let m = operators::parse_ping("67.213.125.125", "sydney", 100, raw);
    assert_eq!(m.min_ms, Some(0.521));
    raw = serde_json::from_value(record["response"].clone()).unwrap();
    raw.cached = true;
    assert!(
        operators::parse_ping("67.213.125.125", "sydney", 100, raw)
            .min_ms
            .is_none()
    );
    raw = serde_json::from_value(record["response"].clone()).unwrap();
    assert!(
        operators::parse_ping("67.213.125.51", "sydney", 100, raw)
            .min_ms
            .is_none()
    );
}
#[test]
fn measurement_can_fix_consensus_but_not_a_neighbour_or_stale_result() {
    let mut e = Entry::default();
    let mut base = point("Nyngan", "AU", -31.2532, 146.921);
    base.asn = Some("AS396356".into());
    e.observations.insert("ip-api".into(), base);
    decide(&mut e);
    assert_eq!(operators::candidates(&e)[0].id, "sydney");
    e.measurements.push(Measurement {
        target: "67.213.125.125".into(),
        vantage: "sydney".into(),
        at: 100,
        min_ms: Some(0.521),
        output: String::new(),
    });
    assert!(!operators::apply_measurements(
        "67.213.125.51",
        &mut e,
        101,
        DAY
    ));
    assert!(!operators::apply_measurements(
        "67.213.125.125",
        &mut e,
        100 + DAY,
        DAY
    ));
    assert!(operators::apply_measurements(
        "67.213.125.125",
        &mut e,
        101,
        DAY
    ));
    assert_eq!(e.decision.as_ref().unwrap().city, "Sydney");
    assert_eq!(e.confidence, "measured_metro");
}
#[test]
fn contradictory_short_rtts_do_not_choose_an_arbitrary_city() {
    let mut e = Entry::default();
    e.observations
        .insert("ip-api".into(), point("New York", "US", 40.7, -74.0));
    decide(&mut e);
    for vantage in ["new_york", "los_angeles"] {
        e.measurements.push(Measurement {
            target: "64.34.88.165".into(),
            vantage: vantage.into(),
            at: 100,
            min_ms: Some(1.0),
            output: String::new(),
        });
    }
    assert!(!operators::apply_measurements(
        "64.34.88.165",
        &mut e,
        101,
        DAY
    ));
    assert_eq!(e.confidence, "disputed");
}
#[test]
fn quota_backoff_and_clock_rollback_survive_serialization() {
    let cfg = ResearchConfig {
        daily_requests: 2,
        daily_measurements: 1,
        ..ResearchConfig::default()
    };
    let mut b = Budget::default();
    let now = 2_000_000_000;
    assert!(b.reserve("latitude-ping", now, &cfg));
    b = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
    assert!(!b.reserve("latitude-ping", now + 5, &cfg));
    assert!(b.reserve("ip-api", now + 5, &cfg));
    assert!(!b.reserve("ip-api", now + 10, &cfg));
    assert!(!b.reserve("ip-api", now - DAY, &cfg));
    assert!(b.reserve("ip-api", now + DAY, &cfg));
    let mut e = Entry::default();
    for delay in [3600, 21600, DAY, 3 * DAY, 7 * DAY, 7 * DAY] {
        e.retry(now);
        assert_eq!(e.next_attempt_at, now + delay);
        assert!(!e.due(now + 300, 90));
    }
}
#[test]
fn geofeed_uses_exact_prefix_and_rejects_region_as_city() {
    let rows = sources::parse_feed(
        "67.213.0.0/16,US,US-NY,New York,\n67.213.125.0/24,AU,AU-NSW,Sydney,\n91.242.215.0/24,AU,AU-NSW,New South Wales,",
        false,
    );
    assert_eq!(
        sources::feed_match(&rows, "67.213.125.125".parse().unwrap())
            .unwrap()
            .city,
        "Sydney"
    );
    assert!(sources::feed_match(&rows, "91.242.215.2".parse().unwrap()).is_none());
    let v6 = sources::parse_feed("2605:6440::/32,US,US-VA,Ashburn,", false);
    assert!(sources::feed_match(&v6, "2605:6440::1".parse().unwrap()).is_some());
}

struct Fixture {
    config: NodeLocationsConfig,
    calls: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
    root: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
async fn fixture(fail: bool) -> Fixture {
    crate::tls::install_rustls_crypto_provider();
    let root = std::env::temp_dir().join(format!("geo-research-test-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&root).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let a = calls.clone();
    let b = calls.clone();
    let c = calls.clone();
    let app=axum::Router::new().route("/batch",axum::routing::post(move |axum::Json(ips):axum::Json<Vec<String>>| {
        let a=a.clone();async move { a.fetch_add(1,Ordering::SeqCst);
            axum::Json(ips.into_iter().map(|ip| if fail { json!({"query":ip,"status":"fail"}) } else {
                json!({"query":ip,"status":"success","city":"Amsterdam","country":"Netherlands","countryCode":"NL","lat":52.36,"lon":4.9,"isp":"fixture","as":"AS64500 fixture"})
            }).collect::<Vec<_>>()) }
    })).route("/{ip}",axum::routing::get(move || { let b=b.clone();async move { b.fetch_add(1,Ordering::SeqCst);axum::Json(json!({"success":false})) } }));
    let app = app.route("/lite/{ip}", axum::routing::get(move |axum::extract::Path(ip): axum::extract::Path<String>| {
        let c = c.clone(); async move {
            c.fetch_add(1,Ordering::SeqCst);
            axum::Json(json!({"ip":ip,"country":"Netherlands","country_code":"NL","asn":"AS64500"}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let config = NodeLocationsConfig {
        geo_cache_path: root.join("cache.json"),
        ip_api_batch_endpoint: format!("http://{address}/batch"),
        tiebreak_base_url: format!("http://{address}"),
        ipinfo_lite_base_url: format!("http://{address}/lite"),
        ipinfo_token: Some("local-test-fixture".into()),
        research: ResearchConfig {
            download_database: false,
            operator_measurements: false,
            ..ResearchConfig::default()
        },
        ..NodeLocationsConfig::default()
    };
    Fixture {
        config,
        calls,
        server,
        root,
    }
}
#[tokio::test]
async fn same_set_for_thirty_days_restart_set_switch_and_return_do_not_relookup_known_ips() {
    let f = fixture(false).await;
    let mut engine = Engine::open(&f.config).unwrap();
    let mut cache = GeoCache::default();
    let now = 2_000_000_000;
    let a = "1.1.1.1".parse().unwrap();
    let b = "8.8.8.8".parse().unwrap();
    engine
        .refresh(&f.config, &[a], &BTreeMap::new(), &mut cache, now)
        .await
        .unwrap();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    for i in 1..=30 * 288 {
        engine
            .refresh(&f.config, &[a], &BTreeMap::new(), &mut cache, now + i * 300)
            .await
            .unwrap();
    }
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    drop(engine);
    let mut engine = Engine::open(&f.config).unwrap();
    let later = now + 31 * DAY;
    engine
        .refresh(&f.config, &[a, b], &BTreeMap::new(), &mut cache, later)
        .await
        .unwrap();
    assert_eq!(f.calls.load(Ordering::SeqCst), 2);
    // Another chain and green->blue->green with an IP already researched.
    engine
        .refresh(&f.config, &[b], &BTreeMap::new(), &mut cache, later + 300)
        .await
        .unwrap();
    cache.locations.clear();
    engine
        .refresh(&f.config, &[a], &BTreeMap::new(), &mut cache, later + 600)
        .await
        .unwrap();
    assert!(cache.location(a).is_some());
    assert_eq!(f.calls.load(Ordering::SeqCst), 2);
    // A deliberate 90-day expiry does permit a new observation.
    engine
        .refresh(
            &f.config,
            &[a],
            &BTreeMap::new(),
            &mut cache,
            now + 90 * DAY,
        )
        .await
        .unwrap();
    assert_eq!(f.calls.load(Ordering::SeqCst), 3);
}
#[tokio::test]
async fn failures_are_negative_cached_across_restarts() {
    let f = fixture(true).await;
    let mut e = Engine::open(&f.config).unwrap();
    let mut cache = GeoCache::default();
    let ip = "1.1.1.1".parse().unwrap();
    let now = 2_000_000_000;
    e.refresh(&f.config, &[ip], &BTreeMap::new(), &mut cache, now)
        .await
        .unwrap();
    let first = f.calls.load(Ordering::SeqCst);
    assert_eq!(first, 2);
    drop(e);
    let mut e = Engine::open(&f.config).unwrap();
    for i in 1..12 {
        e.refresh(
            &f.config,
            &[ip],
            &BTreeMap::new(),
            &mut cache,
            now + i * 300,
        )
        .await
        .unwrap();
    }
    assert_eq!(f.calls.load(Ordering::SeqCst), first);
    e.refresh(&f.config, &[ip], &BTreeMap::new(), &mut cache, now + 3600)
        .await
        .unwrap();
    assert_eq!(f.calls.load(Ordering::SeqCst), first + 2);
    assert_eq!(
        e.store.entries["1.1.1.1"].next_attempt_at,
        now + 3600 + 21600
    );
}
#[tokio::test]
async fn corrupt_journal_or_unwritable_quota_prevents_network_calls() {
    let f = fixture(false).await;
    let mut e = Engine::open(&f.config).unwrap();
    std::fs::write(&e.path, b"broken").unwrap();
    assert!(Engine::open(&f.config).is_err());
    std::fs::remove_file(&e.path).unwrap();
    std::fs::create_dir(&e.path).unwrap();
    let result = e
        .refresh(
            &f.config,
            &["1.1.1.1".parse().unwrap()],
            &BTreeMap::new(),
            &mut GeoCache::default(),
            2_000_000_000,
        )
        .await;
    assert!(result.is_err());
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}

/// Run the SAME decision functions used in production on saved public observations.
/// No network, no embedded IP-specific overrides; direct target measurements remain explicit inputs.
#[test]
#[ignore = "needs locally downloaded production snapshots and free DB-IP MMDB"]
fn compare_saved_production_snapshot() {
    let root = PathBuf::from(
        std::env::var("VALIDATORCLOCK_GEO_AUDIT_DIR").expect("set snapshot directory"),
    );
    let cache: GeoCache =
        super::super::geo_cache::load_geo_cache(&root.join("geo_cache.json")).unwrap();
    let primary: Vec<Value> =
        serde_json::from_slice(&std::fs::read(root.join("fresh-all-ip-api.json")).unwrap())
            .unwrap();
    let primary = primary
        .into_iter()
        .map(|r| (r["query"].as_str().unwrap().to_owned(), r))
        .collect::<BTreeMap<_, _>>();
    let reader = sources::open_database(&root.join("dbip-city-lite.mmdb")).unwrap();
    let recorded: Value = serde_json::from_str(include_str!(
        "../../../docs/geolocation-audit-2026-09-10/observations.json"
    ))
    .unwrap();
    let mut ips = BTreeMap::new();
    for name in [
        "ton_map/ton_nodes.json",
        "everscale_map/everscale_nodes.json",
        "tycho_map/tycho_nodes_native.json",
    ] {
        let nodes: Vec<Value> =
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
        for n in nodes {
            ips.insert(n["ip"].as_str().unwrap().to_owned(), n);
        }
    }
    let mut rows = vec![];
    let now = 1_789_001_000;
    let mut moved = 0;
    let mut disputed = 0;
    let mut measured = 0;
    let config = NodeLocationsConfig {
        geo_cache_path: root.join("offline-benchmark/geo.json"),
        ..NodeLocationsConfig::default()
    };
    let mut engine = Engine::open(&config).unwrap();
    for (provider, name) in [
        ("hetzner", "hetzner-geofeed.txt"),
        ("latitude", "latitude-geofeed.txt"),
    ] {
        if let Ok(body) = std::fs::read_to_string(root.join(name)) {
            engine.feeds.insert(
                provider.into(),
                sources::parse_feed(&body, provider == "latitude"),
            );
        }
    }
    for (ip, node) in ips {
        let address = ip.parse().unwrap();
        let mut e = Entry::default();
        if let Some(value) = primary
            .get(&ip)
            .and_then(|r| sources::observation(r, "ip-api", now))
        {
            e.observations.insert("ip-api".into(), value);
        }
        if let Some(mut db) = sources::database_observation(&reader, address, now) {
            db.asn = cache.location(address).and_then(|c| c.asn.clone());
            db.isp = cache
                .location(address)
                .map(|c| c.isp.clone())
                .unwrap_or_default();
            e.observations.insert("dbip".into(), db);
        }
        if let Some(old) = cache.location(address)
            && old.source == "ipwho.is"
        {
            e.observations
                .insert("ipwho.is".into(), Observation::from_cached(old));
        }
        if let Some(record) = recorded["lookups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["ip"] == ip)
            && let Some(point) = sources::observation(&record["fresh_ipwho"], "ipwho.is", now)
        {
            e.observations.insert("ipwho.is".into(), point);
        }
        decide(&mut e);
        engine.apply_feed(address, &mut e);
        let proposed = operators::candidates(&e);
        if !proposed.is_empty() && e.confidence == "approximate" {
            e.confidence = "disputed".into();
        }
        for r in recorded["looking_glass"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["request"]["query_target"] == ip)
        {
            let vantage = r["request"]["query_location"].as_str().unwrap();
            if proposed.iter().any(|m| m.id == vantage) {
                let raw: operators::PingResponse =
                    serde_json::from_value(r["response"].clone()).unwrap();
                e.measurements
                    .push(operators::parse_ping(&ip, vantage, now, raw));
            }
        }
        operators::apply_measurements(&ip, &mut e, now, 90 * DAY);
        // Preserve production manual overrides; the runtime filters them before research.
        if node["geo_source"] == "manual" {
            let mut manual = point(
                node["city"].as_str().unwrap(),
                cache
                    .location(address)
                    .unwrap()
                    .country_code
                    .as_deref()
                    .unwrap(),
                node["lat"].as_f64().unwrap(),
                node["lon"].as_f64().unwrap(),
            );
            manual.country = node["country"].as_str().unwrap().into();
            manual.source = "manual".into();
            e.decision = Some(manual);
            e.confidence = "manual".into();
        }
        let Some(selected) = &e.decision else {
            panic!("unmapped {ip}");
        };
        let mut old = point(
            node["city"].as_str().unwrap(),
            "XX",
            node["lat"].as_f64().unwrap(),
            node["lon"].as_f64().unwrap(),
        );
        old.country = node["country"].as_str().unwrap().into();
        let km = model::distance(&old, selected);
        if km > 100.0 {
            moved += 1;
        }
        if e.confidence == "disputed" {
            disputed += 1;
        }
        if e.confidence == "measured_metro" {
            measured += 1;
        }
        rows.push(json!({"ip":ip,"old":node,"new":selected,"confidence":e.confidence,"reasons":e.reasons,"moved_km":km,"proposed_measurement_vantages":proposed}));
    }
    let report = json!({"method":"Current Rust decision functions; fresh ip-api for all IPs, free DB-IP, two operator feeds, seven saved secondary lookups and saved direct target pings. No general ground truth; data-date changes are not algorithm accuracy gains.","summary":{"ips":rows.len(),"moved_over_100km":moved,"disputed":disputed,"measured_metros":measured},"rows":rows});
    std::fs::write(
        root.join("rust-comparison.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", report["summary"]);
    assert_eq!(report["summary"]["ips"], 416);
    assert_eq!(measured, 2);
    for (ip, city) in [
        ("67.213.125.125", "Sydney"),
        ("64.34.88.165", "Los Angeles"),
    ] {
        let r = report["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["ip"] == ip)
            .unwrap();
        assert_eq!(r["new"]["city"], city);
    }
}

/// Explicit opt-in trial of the actual async engine. Writes only under the supplied
/// snapshot directory. Reuses already-downloaded official assets, never production files.
#[tokio::test]
#[ignore = "explicit live trial; uses bounded free-service queries"]
async fn live_snapshot_trial() {
    crate::tls::install_rustls_crypto_provider();
    let root = PathBuf::from(
        std::env::var("VALIDATORCLOCK_GEO_AUDIT_DIR").expect("set snapshot directory"),
    );
    let now = crate::timeutil::now_sec();
    let config = NodeLocationsConfig {
        geo_cache_path: root.join("live-trial/cache.json"),
        research: ResearchConfig {
            max_ips_per_cycle: 500,
            ..ResearchConfig::default()
        },
        ..NodeLocationsConfig::default()
    };
    let mut engine = Engine::open(&config).unwrap();
    if !engine.database_path().exists() {
        std::fs::copy(root.join("dbip-city-lite.mmdb"), engine.database_path()).unwrap();
    }
    let month = crate::timeutil::day_string(crate::timeutil::day_index(now))[..7].to_owned();
    engine.store.assets.insert(format!("dbip-{month}"), now);
    for (name, source) in [
        ("hetzner", "hetzner-geofeed.txt"),
        ("latitude", "latitude-geofeed.txt"),
    ] {
        std::fs::copy(
            root.join(source),
            engine.directory.join(format!("{name}.csv")),
        )
        .unwrap();
        engine.store.assets.insert(name.into(), now + 30 * DAY);
    }
    let mut cache = super::super::geo_cache::load_geo_cache(&root.join("geo_cache.json")).unwrap();
    let mut nodes = BTreeMap::new();
    let mut manual = BTreeMap::new();
    for name in [
        "ton_map/ton_nodes.json",
        "everscale_map/everscale_nodes.json",
        "tycho_map/tycho_nodes_native.json",
    ] {
        let values: Vec<Value> =
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
        for n in values {
            let ip: IpAddr = n["ip"].as_str().unwrap().parse().unwrap();
            if n["geo_source"] == "manual" {
                manual.insert(ip,serde_json::from_value::<ManualResolvedIp>(json!({"ip":ip.to_string(),"geo":{"city":n["city"],"country":n["country"],"latitude":n["lat"],"longitude":n["lon"]}})).unwrap());
            }
            nodes.insert(ip, n);
        }
    }
    let ips = nodes.keys().copied().collect::<Vec<_>>();
    let before = engine.store.budget.total_requests;
    engine
        .refresh(&config, &ips, &manual, &mut cache, now)
        .await
        .unwrap();
    let after = engine.store.budget.total_requests;
    engine
        .refresh(&config, &ips, &manual, &mut cache, now + 300)
        .await
        .unwrap();
    let warm = engine.store.budget.total_requests - after;
    drop(engine);
    let mut engine = Engine::open(&config).unwrap();
    engine
        .refresh(&config, &ips, &manual, &mut cache, now + 600)
        .await
        .unwrap();
    let restarted = engine.store.budget.total_requests - after - warm;
    let mut moved = 0;
    let mut disputed = 0;
    let mut measured = 0;
    let rows=nodes.iter().map(|(ip,old)| {
        let new=cache.location(*ip).unwrap();
        let distance=if manual.contains_key(ip) {0.0} else {model::distance(&point("","XX",old["lat"].as_f64().unwrap(),old["lon"].as_f64().unwrap()),&Observation::from_cached(new))};
        if distance>100.0 { moved+=1; }
        if new.confidence=="disputed" { disputed+=1; }
        if new.confidence=="measured_metro" { measured+=1; }
        json!({"ip":ip.to_string(),"old_city":old["city"],"old_country":old["country"],"new_city":if manual.contains_key(ip) {old["city"].clone()} else {json!(new.city)},"new_country":new.country,"confidence":new.confidence,"moved_km":distance})
    }).collect::<Vec<_>>();
    let report = json!({"observed_at":now,"summary":{"ips":ips.len(),"first_pass_http_requests":after-before,"same_set_after_five_minutes_http_requests":warm,"after_restart_http_requests":restarted,"moved_over_100km":moved,"disputed":disputed,"measured_metros":measured},"notes":"Actual async engine on a COPY of production node cache; official database/feed files preseeded from this audit. Their download cost is not included. Manual overrides preserved. No production changes.","rows":rows});
    std::fs::write(
        root.join("live-trial-report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", report["summary"]);
    assert_eq!(warm, 0);
    assert_eq!(restarted, 0);
}

#[tokio::test]
async fn provider_rate_limit_is_global_and_persisted() {
    let f = fixture(false).await;
    let mut engine = Engine::open(&f.config).unwrap();
    let now = 2_000_000_000;
    let response = reqwest::Response::from(
        axum::http::Response::builder()
            .status(429)
            .header("retry-after", "3600")
            .body(String::new())
            .unwrap(),
    );
    engine.rate_headers("ipwho.is", &response, now).unwrap();
    drop(engine);
    let mut engine = Engine::open(&f.config).unwrap();
    assert!(!engine.reserve("ipwho.is", now + 300).unwrap());
    assert!(engine.reserve("ipwho.is", now + 3600).unwrap());
    let exhausted = reqwest::Response::from(
        axum::http::Response::builder()
            .header("x-rl", "0")
            .header("x-ttl", "60")
            .body(String::new())
            .unwrap(),
    );
    engine
        .rate_headers("ip-api", &exhausted, now + 3600)
        .unwrap();
    assert!(!engine.reserve("ip-api", now + 3659).unwrap());
    assert!(engine.reserve("ip-api", now + 3660).unwrap());
}

#[tokio::test]
async fn truncated_http_body_is_a_retryable_miss_not_a_worker_error() {
    use tokio::io::AsyncWriteExt;
    let f = fixture(false).await;
    let mut engine = Engine::open(&f.config).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
    });
    let body = engine
        .get_bytes(
            "ipwho.is",
            &format!("http://{addr}/"),
            2_000_000_000,
            1024,
            5,
        )
        .await;
    assert!(body.unwrap().is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn old_and_new_request_counts_for_a_successful_ip_over_thirty_days() {
    let f = fixture(false).await;
    let ip = "1.1.1.1".parse().unwrap();
    let now = 2_000_000_000;
    let mut old = GeoCache::default();
    for day in [0, 7, 14, 21, 28] {
        super::super::legacy_refresh(
            crate::http::shared_client(),
            &f.config,
            &[ip],
            &BTreeMap::new(),
            &mut old,
            now + day * DAY,
            Duration::from_secs(7 * DAY),
        )
        .await;
    }
    let legacy_calls = f.calls.load(Ordering::SeqCst);
    assert_eq!(legacy_calls, 10);
    let mut engine = Engine::open(&f.config).unwrap();
    let mut new = GeoCache::default();
    for day in 0..30 {
        engine
            .refresh(
                &f.config,
                &[ip],
                &BTreeMap::new(),
                &mut new,
                now + day * DAY,
            )
            .await
            .unwrap();
    }
    assert_eq!(f.calls.load(Ordering::SeqCst) - legacy_calls, 1);
    println!(
        "30-day successful IP, per-IP HTTP only: old=10 new=1 (asset downloads disabled in fixture)"
    );
}
#[tokio::test]
async fn old_and_new_request_counts_for_an_unresolved_ip_over_one_day() {
    let f = fixture(true).await;
    let ip = "1.1.1.1".parse().unwrap();
    let now = 2_000_000_000;
    let mut old = GeoCache::default();
    for tick in 0..288 {
        super::super::legacy_refresh(
            crate::http::shared_client(),
            &f.config,
            &[ip],
            &BTreeMap::new(),
            &mut old,
            now + tick * 300,
            Duration::from_secs(7 * DAY),
        )
        .await;
    }
    let legacy_calls = f.calls.load(Ordering::SeqCst);
    assert_eq!(legacy_calls, 288);
    let mut engine = Engine::open(&f.config).unwrap();
    let mut new = GeoCache::default();
    for tick in 0..288 {
        engine
            .refresh(
                &f.config,
                &[ip],
                &BTreeMap::new(),
                &mut new,
                now + tick * 300,
            )
            .await
            .unwrap();
    }
    assert_eq!(f.calls.load(Ordering::SeqCst) - legacy_calls, 6);
    println!("24-hour unresolved IP: old=288 new=6 (local HTTP fixture only)");
}

#[tokio::test]
async fn migration_does_not_treat_an_old_cache_without_iso_code_as_verified() {
    let f = fixture(false).await;
    let mut e = Engine::open(&f.config).unwrap();
    let ip: IpAddr = "1.1.1.1".parse().unwrap();
    let now = 2_000_000_000;
    let mut old = point("Amsterdam", "NL", 52.36, 4.9).cached("medium");
    old.country_code = None;
    let mut cache = GeoCache::default();
    cache.locations.insert(ip.to_string(), old);
    e.refresh(&f.config, &[ip], &BTreeMap::new(), &mut cache, now)
        .await
        .unwrap();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        cache.location(ip).unwrap().country_code.as_deref(),
        Some("NL")
    );
    cache.location_mut(ip).unwrap().country = "wrong".into();
    e.refresh(&f.config, &[ip], &BTreeMap::new(), &mut cache, now + 300)
        .await
        .unwrap();
    assert_eq!(cache.location(ip).unwrap().country, "Netherlands");
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}
