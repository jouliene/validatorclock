use super::*;
use axum::{Router, routing::get};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tycho_network::{Address, PeerId};

struct Fixture {
    dir: PathBuf,
    url: String,
    body: Arc<std::sync::Mutex<String>>,
    calls: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        crate::tls::install_rustls_crypto_provider();
        let dir =
            std::env::temp_dir().join(format!("tycho-bootstrap-{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let body = Arc::new(std::sync::Mutex::new(String::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let response = Arc::clone(&body);
        let requests = Arc::clone(&calls);
        let app = Router::new().route(
            "/global.json",
            get(move || {
                requests.fetch_add(1, Ordering::Relaxed);
                let body = response.lock().unwrap().clone();
                async move { body }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/global.json", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            dir,
            url,
            body,
            calls,
            server,
        }
    }

    fn source(&self) -> BootstrapSource {
        BootstrapSource::new(&self.dir.join("global.json"), Some(&self.url))
    }

    fn serve_peer(&self, peer: &PeerInfo) {
        *self.body.lock().unwrap() = config_body(peer);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn peer(ip: &str) -> PeerInfo {
    let secret = tycho_crypto::ed25519::SecretKey::from_bytes([17; 32]);
    let key = tycho_crypto::ed25519::KeyPair::from(&secret);
    let mut peer = PeerInfo {
        id: PeerId::from(key.public_key),
        address_list: vec![Address::Ip {
            ip: ip.parse().unwrap(),
            port: 30000,
        }]
        .into_boxed_slice(),
        created_at: crate::timeutil::now_sec() as u32 - 10,
        expires_at: u32::MAX,
        signature: Box::new([0; 64]),
    };
    peer.signature = Box::new(key.sign_tl(&peer));
    peer
}

fn config_body(peer: &PeerInfo) -> String {
    serde_json::json!({"bootstrap_peers": [peer]}).to_string()
}

#[tokio::test]
async fn moved_bootstrap_addresses_replace_old_file_and_survive_restart_offline() {
    let fixture = Fixture::new().await;
    let old = peer("192.0.2.1");
    let moved = peer("192.0.2.2");
    let source = fixture.source();
    std::fs::write(&source.path, config_body(&old)).unwrap();
    fixture.serve_peer(&moved);

    assert_eq!(
        source.load().await.unwrap().as_slice(),
        std::slice::from_ref(&moved)
    );
    assert_eq!(read_bootstrap_peers(&source.path).unwrap(), [old]);
    assert!(source.refresh().await.is_none());
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);

    fixture.server.abort();
    assert_eq!(fixture.source().load().await.unwrap(), [moved]);
}

#[tokio::test]
async fn periodic_refresh_picks_up_moves_and_invalid_responses_preserve_cache() {
    let fixture = Fixture::new().await;
    let source = fixture.source();
    let first = peer("192.0.2.1");
    fixture.serve_peer(&first);
    source.load().await.unwrap();

    let moved = peer("192.0.2.2");
    fixture.serve_peer(&moved);
    *source.next_refresh.lock().await = Instant::now();
    assert_eq!(
        source.refresh().await.unwrap().as_slice(),
        std::slice::from_ref(&moved)
    );
    let cached = std::fs::read(source.cache_path()).unwrap();

    let mut forged = peer("192.0.2.3");
    forged.signature[0] ^= 1;
    for body in [
        config_body(&forged),
        "{}".to_owned(),
        "<html>error</html>".to_owned(),
        "x".repeat(MAX_CONFIG_BYTES + 1),
    ] {
        *fixture.body.lock().unwrap() = body;
        *source.next_refresh.lock().await = Instant::now();
        assert!(source.refresh().await.is_none());
        assert_eq!(std::fs::read(source.cache_path()).unwrap(), cached);
        let calls = fixture.calls.load(Ordering::Relaxed);
        assert!(source.refresh().await.is_none());
        assert_eq!(
            fixture.calls.load(Ordering::Relaxed),
            calls,
            "failed sources must back off"
        );
    }
    assert_eq!(fixture.source().load().await.unwrap(), [moved]);
}

#[tokio::test]
async fn unavailable_source_falls_back_to_local_file_without_using_another_sources_cache() {
    let fixture = Fixture::new().await;
    let source = fixture.source();
    let local = peer("192.0.2.1");
    std::fs::write(&source.path, config_body(&local)).unwrap();
    fixture.serve_peer(&peer("192.0.2.2"));
    source.load().await.unwrap();
    let other = BootstrapSource::new(
        &source.path,
        Some(&format!("{}?different-network", fixture.url)),
    );
    fixture.server.abort();
    assert_eq!(
        other.load().await.unwrap().as_slice(),
        std::slice::from_ref(&local)
    );
    let file_only = BootstrapSource::new(&source.path, None);
    assert_eq!(file_only.load().await.unwrap(), [local]);
}

#[test]
fn unsigned_expired_empty_or_malformed_configs_are_rejected() {
    assert!(validate_bootstrap_peers(vec![]).is_err());
    let mut invalid = peer("192.0.2.1");
    invalid.signature[0] ^= 1;
    assert!(validate_bootstrap_peers(vec![invalid]).is_err());
    let mut expired = peer("192.0.2.1");
    expired.expires_at = 1;
    assert!(validate_bootstrap_peers(vec![expired]).is_err());
    assert!(serde_json::from_str::<TychoGlobalConfig>("{ not json").is_err());
}
