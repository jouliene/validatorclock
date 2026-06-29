use super::{SecurityConfig, TlsConfig};
use crate::history::round_history_chain_path;
use crate::server;
use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AppConfig {
    #[serde(default = "default_listen")]
    pub(crate) listen: String,
    #[serde(default = "default_refresh_seconds")]
    pub(crate) refresh_seconds: u64,
    #[serde(default = "default_refresh_timeout_seconds")]
    pub(crate) refresh_timeout_seconds: u64,
    #[serde(default = "default_cache_path")]
    pub(crate) cache_path: PathBuf,
    #[serde(default)]
    pub(crate) analytics_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) history_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) tycho_map_nodes_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) map_nodes_paths: HashMap<String, PathBuf>,
    #[serde(default)]
    pub(crate) node_locations: NodeLocationsConfig,
    #[serde(default)]
    pub(crate) security: SecurityConfig,
    #[serde(default)]
    pub(crate) tls: TlsConfig,
    pub(crate) chains: Vec<ChainConfig>,
}

impl AppConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.chains.is_empty() {
            bail!("config must contain at least one chain");
        }
        if self.refresh_seconds == 0 {
            bail!("refresh_seconds must be greater than zero");
        }
        if self.refresh_timeout_seconds == 0 {
            bail!("refresh_timeout_seconds must be greater than zero");
        }
        if self
            .analytics_path
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            bail!("analytics_path cannot be empty when set");
        }
        if self
            .history_path
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            bail!("history_path cannot be empty when set");
        }
        if self
            .tycho_map_nodes_path
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            bail!("tycho_map_nodes_path cannot be empty when set");
        }
        for (chain_id, path) in &self.map_nodes_paths {
            if chain_id.trim().is_empty() {
                bail!("map_nodes_paths cannot contain an empty chain id");
            }
            if path.as_os_str().is_empty() {
                bail!("map_nodes_paths entry for `{chain_id}` cannot be empty");
            }
        }

        self.node_locations.validate()?;
        for chain in &self.chains {
            chain.validate()?;
        }
        self.validate_chain_ids()?;
        self.validate_history_paths()?;

        self.security.validate()?;
        self.tls.validate()?;
        Ok(())
    }

    pub(crate) fn chain(&self, id: &str) -> Option<&ChainConfig> {
        self.chains.iter().find(|chain| chain.id == id)
    }

    pub(crate) fn effective_allowed_hosts(&self) -> Vec<String> {
        let mut hosts = self.security.allowed_hosts.clone();
        if self.tls.enabled
            && let Some(host) = server::public_url_host(&self.tls.public_url)
            && !hosts.iter().any(|item| item == &host)
        {
            hosts.push(host);
        }
        hosts
    }

    pub(crate) fn effective_history_path(&self) -> PathBuf {
        self.history_path.clone().unwrap_or_else(|| {
            let mut path = self.cache_path.clone();
            path.set_file_name("validatorclock_history.json");
            path
        })
    }

    pub(crate) fn effective_analytics_path(&self) -> PathBuf {
        self.analytics_path.clone().unwrap_or_else(|| {
            let mut path = self.cache_path.clone();
            path.set_file_name("validatorclock_analytics.json");
            path
        })
    }

    pub(crate) fn effective_validator_type_cache_path(&self) -> PathBuf {
        let mut path = self.cache_path.clone();
        path.set_file_name("validatorclock_validator_types.json");
        path
    }

    pub(crate) fn effective_node_location_chain(&self, chain_id: &str) -> NodeLocationChainConfig {
        self.node_locations.effective_chain_config(chain_id)
    }

    pub(crate) fn node_location_output_path(&self, chain_id: &str) -> Option<PathBuf> {
        if !self.node_locations.enabled {
            return None;
        }
        let chain = self.effective_node_location_chain(chain_id);
        (chain.enabled && !chain.output_path.as_os_str().is_empty()).then_some(chain.output_path)
    }

    fn validate_chain_ids(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for chain in &self.chains {
            if chain.id.trim() != chain.id {
                bail!(
                    "chain id `{}` cannot contain leading or trailing whitespace",
                    chain.id
                );
            }
            if !chain
                .id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            {
                bail!(
                    "chain id `{}` can only contain ASCII letters, digits, `-`, and `_`",
                    chain.id
                );
            }
            if !seen.insert(chain.id.as_str()) {
                bail!("duplicate chain id `{}`", chain.id);
            }
        }
        Ok(())
    }

    fn validate_history_paths(&self) -> Result<()> {
        let history_base_path = self.effective_history_path();
        let mut paths = HashMap::<PathBuf, &str>::new();
        for chain in &self.chains {
            let path = round_history_chain_path(&history_base_path, &chain.id);
            if let Some(existing_chain_id) = paths.insert(path.clone(), &chain.id) {
                bail!(
                    "chains `{}` and `{}` derive the same history path {}",
                    existing_chain_id,
                    chain.id,
                    path.display()
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NodeLocationsConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default = "default_node_locations_refresh_seconds")]
    pub(crate) refresh_seconds: u64,
    #[serde(default = "default_node_locations_startup_delay_seconds")]
    pub(crate) startup_delay_seconds: u64,
    #[serde(default = "default_node_locations_geo_cache_path")]
    pub(crate) geo_cache_path: PathBuf,
    #[serde(default = "default_node_locations_geo_cache_ttl_seconds")]
    pub(crate) geo_cache_ttl_seconds: u64,
    #[serde(default = "default_ip_api_batch_endpoint")]
    pub(crate) ip_api_batch_endpoint: String,
    #[serde(default)]
    pub(crate) ipinfo_token: Option<String>,
    #[serde(default = "default_ipinfo_token_env")]
    pub(crate) ipinfo_token_env: String,
    #[serde(default = "default_ipinfo_lite_base_url")]
    pub(crate) ipinfo_lite_base_url: String,
    #[serde(default = "default_manual_review_dir")]
    pub(crate) manual_review_dir: PathBuf,
    #[serde(default = "default_manual_resolved_dir")]
    pub(crate) manual_resolved_dir: PathBuf,
    #[serde(default = "default_node_location_chains")]
    pub(crate) chains: HashMap<String, NodeLocationChainConfig>,
}

impl Default for NodeLocationsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            refresh_seconds: default_node_locations_refresh_seconds(),
            startup_delay_seconds: default_node_locations_startup_delay_seconds(),
            geo_cache_path: default_node_locations_geo_cache_path(),
            geo_cache_ttl_seconds: default_node_locations_geo_cache_ttl_seconds(),
            ip_api_batch_endpoint: default_ip_api_batch_endpoint(),
            ipinfo_token: None,
            ipinfo_token_env: default_ipinfo_token_env(),
            ipinfo_lite_base_url: default_ipinfo_lite_base_url(),
            manual_review_dir: default_manual_review_dir(),
            manual_resolved_dir: default_manual_resolved_dir(),
            chains: default_node_location_chains(),
        }
    }
}

impl NodeLocationsConfig {
    fn validate(&self) -> Result<()> {
        if self.refresh_seconds == 0 {
            bail!("node_locations.refresh_seconds must be greater than zero");
        }
        if self.geo_cache_path.as_os_str().is_empty() {
            bail!("node_locations.geo_cache_path cannot be empty");
        }
        if self.geo_cache_ttl_seconds == 0 {
            bail!("node_locations.geo_cache_ttl_seconds must be greater than zero");
        }
        if self.ip_api_batch_endpoint.trim().is_empty() {
            bail!("node_locations.ip_api_batch_endpoint cannot be empty");
        }
        if self.ipinfo_token_env.trim().is_empty() {
            bail!("node_locations.ipinfo_token_env cannot be empty");
        }
        if self.ipinfo_lite_base_url.trim().is_empty() {
            bail!("node_locations.ipinfo_lite_base_url cannot be empty");
        }
        if self.manual_review_dir.as_os_str().is_empty() {
            bail!("node_locations.manual_review_dir cannot be empty");
        }
        if self.manual_resolved_dir.as_os_str().is_empty() {
            bail!("node_locations.manual_resolved_dir cannot be empty");
        }
        for chain_id in self.chains.keys() {
            if chain_id.trim().is_empty() {
                bail!("node_locations.chains cannot contain an empty chain id");
            }
            if chain_id.trim() != chain_id {
                bail!("node_locations chain id `{chain_id}` cannot contain surrounding whitespace");
            }
            let effective = self.effective_chain_config(chain_id);
            if !effective.enabled {
                continue;
            }
            let Some(input_path) = &effective.input_path else {
                bail!("node_locations.chains.{chain_id}.input_path is required when enabled");
            };
            if input_path.as_os_str().is_empty() {
                bail!("node_locations.chains.{chain_id}.input_path cannot be empty when enabled");
            }
            if effective.output_path.as_os_str().is_empty() {
                bail!("node_locations.chains.{chain_id}.output_path cannot be empty when enabled");
            }
            if input_path == &effective.output_path {
                bail!("node_locations.chains.{chain_id}.input_path must differ from output_path");
            }
        }
        Ok(())
    }

    pub(crate) fn effective_chain_config(&self, chain_id: &str) -> NodeLocationChainConfig {
        let mut effective = default_node_location_chain_config(chain_id);
        if let Some(configured) = self.chains.get(chain_id) {
            effective.enabled = configured.enabled;
            if configured.input_path.is_some() {
                effective.input_path = configured.input_path.clone();
            }
            if !configured.output_path.as_os_str().is_empty() {
                effective.output_path = configured.output_path.clone();
            }
        }
        effective
    }

    pub(crate) fn effective_ipinfo_token(&self) -> Option<String> {
        self.ipinfo_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                std::env::var(&self.ipinfo_token_env)
                    .ok()
                    .map(|token| token.trim().to_owned())
                    .filter(|token| !token.is_empty())
            })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NodeLocationChainConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) input_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) output_path: PathBuf,
}

impl Default for NodeLocationChainConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            input_path: None,
            output_path: PathBuf::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChainConfig {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) rpc: String,
    #[serde(default)]
    pub(crate) rpc_fallbacks: Vec<String>,
    #[serde(default = "default_chain_color")]
    pub(crate) color: String,
    #[serde(default = "default_token_symbol")]
    pub(crate) token_symbol: String,
    #[serde(default)]
    pub(crate) rpc_label: Option<String>,
}

impl ChainConfig {
    fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("chain id cannot be empty");
        }
        if self.name.trim().is_empty() {
            bail!("chain `{}` has empty name", self.id);
        }
        if self.rpc.trim().is_empty() {
            bail!("chain `{}` has empty rpc", self.id);
        }
        if self
            .rpc_fallbacks
            .iter()
            .any(|fallback| fallback.trim().is_empty())
        {
            bail!("chain `{}` has empty rpc fallback", self.id);
        }
        Ok(())
    }
}

fn default_listen() -> String {
    "127.0.0.1:8787".to_owned()
}

fn default_refresh_seconds() -> u64 {
    60
}

fn default_refresh_timeout_seconds() -> u64 {
    90
}

fn default_cache_path() -> PathBuf {
    PathBuf::from("validatorclock_cache.json")
}

fn default_chain_color() -> String {
    "#38bdf8".to_owned()
}

fn default_token_symbol() -> String {
    "TOKENS".to_owned()
}

fn default_node_locations_refresh_seconds() -> u64 {
    300
}

fn default_node_locations_startup_delay_seconds() -> u64 {
    3
}

fn default_node_locations_geo_cache_path() -> PathBuf {
    PathBuf::from(".local_maps/geo_cache.json")
}

fn default_node_locations_geo_cache_ttl_seconds() -> u64 {
    604_800
}

fn default_ip_api_batch_endpoint() -> String {
    "http://ip-api.com/batch?fields=status,message,query,country,countryCode,city,lat,lon,isp,as"
        .to_owned()
}

fn default_ipinfo_token_env() -> String {
    "IPINFO_TOKEN".to_owned()
}

fn default_ipinfo_lite_base_url() -> String {
    "https://api.ipinfo.io/lite".to_owned()
}

fn default_manual_review_dir() -> PathBuf {
    PathBuf::from(".local_maps/manual_review")
}

fn default_manual_resolved_dir() -> PathBuf {
    PathBuf::from(".local_maps/manual_resolved")
}

fn default_node_location_chains() -> HashMap<String, NodeLocationChainConfig> {
    [
        (
            "everscale",
            ".local_maps/raw/everscale_peer_ips.json",
            ".local_maps/everscale_nodes.json",
        ),
        (
            "tycho-testnet",
            ".local_maps/raw/tycho_peer_ips.json",
            ".local_maps/tycho_nodes.json",
        ),
        (
            "ton",
            ".local_maps/raw/ton_peer_ips.json",
            ".local_maps/ton_nodes.json",
        ),
    ]
    .into_iter()
    .map(|(chain_id, input_path, output_path)| {
        (
            chain_id.to_owned(),
            NodeLocationChainConfig {
                enabled: false,
                input_path: Some(PathBuf::from(input_path)),
                output_path: PathBuf::from(output_path),
            },
        )
    })
    .collect()
}

fn default_node_location_chain_config(chain_id: &str) -> NodeLocationChainConfig {
    let mut defaults = default_node_location_chains();
    defaults.remove(chain_id).unwrap_or_default()
}
