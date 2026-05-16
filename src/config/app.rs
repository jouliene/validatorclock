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
    pub(crate) history_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) tycho_map_nodes_path: Option<PathBuf>,
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
            path.set_file_name("validators_clock_history.json");
            path
        })
    }

    pub(crate) fn effective_validator_type_cache_path(&self) -> PathBuf {
        let mut path = self.cache_path.clone();
        path.set_file_name("validators_clock_validator_types.json");
        path
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
pub(crate) struct ChainConfig {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) rpc: String,
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
    PathBuf::from("validators_clock_cache.json")
}

fn default_chain_color() -> String {
    "#38bdf8".to_owned()
}

fn default_token_symbol() -> String {
    "TOKENS".to_owned()
}
