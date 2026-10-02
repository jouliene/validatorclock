//! Refreshable bootstrap addresses; a permanently valid signature does not mean
//! that the peer still lives at the address in an old global config.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{info, warn};
use tycho_network::PeerInfo;

const REFRESH_INTERVAL: Duration = Duration::from_secs(3600);
const RETRY_INTERVAL: Duration = Duration::from_secs(300);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CONFIG_BYTES: usize = 1024 * 1024;

pub(super) struct BootstrapSource {
    path: PathBuf,
    url: Option<String>,
    next_refresh: Mutex<Instant>,
}

impl BootstrapSource {
    pub(super) fn new(path: &Path, url: Option<&str>) -> Self {
        Self {
            path: path.to_owned(),
            url: url.map(str::to_owned),
            next_refresh: Mutex::new(Instant::now()),
        }
    }

    pub(super) async fn load(&self) -> Result<Vec<PeerInfo>> {
        if let Some(peers) = self.refresh().await {
            return Ok(peers);
        }
        if let Some(url) = &self.url {
            match self.read_cache(url) {
                Ok(peers) => {
                    info!("using cached Tycho bootstrap peers");
                    return Ok(peers);
                }
                Err(error) => {
                    warn!(error = %error, "Tycho bootstrap cache unavailable; using local config")
                }
            }
        }
        read_bootstrap_peers(&self.path)
    }

    /// A failed fetch never removes peers already held by the running DHT.
    pub(super) async fn refresh(&self) -> Option<Vec<PeerInfo>> {
        let url = self.url.as_deref()?;
        let mut next = self.next_refresh.lock().await;
        if Instant::now() < *next {
            return None;
        }
        match fetch_bootstrap_peers(url).await {
            Ok(peers) => {
                *next = Instant::now() + REFRESH_INTERVAL;
                let cached = CachedBootstrap {
                    source_hash: source_hash(url),
                    bootstrap_peers: peers.clone(),
                };
                if let Err(error) = serde_json::to_vec(&cached)
                    .context("failed to encode bootstrap cache")
                    .and_then(|body| {
                        crate::fsutil::write_file_atomic(&self.cache_path(), &body, 0o644)
                    })
                {
                    warn!(error = %error, "could not save Tycho bootstrap cache");
                }
                info!(peers = peers.len(), "refreshed Tycho bootstrap peers");
                Some(peers)
            }
            Err(error) => {
                *next = Instant::now() + RETRY_INTERVAL;
                warn!(error = %error, "could not refresh Tycho bootstrap peers; keeping known peers");
                None
            }
        }
    }

    fn cache_path(&self) -> PathBuf {
        // Append rather than replace the extension, so this cannot overwrite
        // the configured file even if it already has this extension.
        let mut path = self.path.as_os_str().to_owned();
        path.push(".bootstrap-cache.json");
        PathBuf::from(path)
    }

    fn read_cache(&self, url: &str) -> Result<Vec<PeerInfo>> {
        let cached: CachedBootstrap = serde_json::from_slice(&std::fs::read(self.cache_path())?)?;
        if cached.source_hash != source_hash(url) {
            bail!("bootstrap cache belongs to a different source");
        }
        validate_bootstrap_peers(cached.bootstrap_peers)
    }
}

fn source_hash(url: &str) -> String {
    hex::encode(Sha256::digest(url.as_bytes()))
}

#[derive(Debug, Deserialize, Serialize)]
struct CachedBootstrap {
    source_hash: String,
    bootstrap_peers: Vec<PeerInfo>,
}

#[derive(Debug, Deserialize)]
struct TychoGlobalConfig {
    #[serde(default)]
    bootstrap_peers: Vec<PeerInfo>,
}

async fn fetch_bootstrap_peers(url: &str) -> Result<Vec<PeerInfo>> {
    let response = crate::http::shared_client()
        .get(url)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.without_url())
        .context("bootstrap request failed")?;
    let config: TychoGlobalConfig = crate::http::json_within(response, MAX_CONFIG_BYTES).await?;
    validate_bootstrap_peers(config.bootstrap_peers)
}

fn read_bootstrap_peers(path: &Path) -> Result<Vec<PeerInfo>> {
    let body = std::fs::read(path)
        .with_context(|| format!("failed to read global config {}", path.display()))?;
    let config: TychoGlobalConfig = serde_json::from_slice(&body)
        .with_context(|| format!("failed to parse global config {}", path.display()))?;
    validate_bootstrap_peers(config.bootstrap_peers)
}

fn validate_bootstrap_peers(peers: Vec<PeerInfo>) -> Result<Vec<PeerInfo>> {
    if peers.is_empty() {
        bail!("global config lists no bootstrap peers");
    }
    let now = crate::timeutil::now_sec() as u32;
    for peer in &peers {
        if !peer.verify(now) {
            bail!("invalid or expired bootstrap peer {}", peer.id);
        }
    }
    Ok(peers)
}

#[cfg(test)]
mod tests;
