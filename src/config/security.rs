use anyhow::{Result, bail};
use serde::Deserialize;

const DEFAULT_MAX_CONNECTIONS: usize = 128;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct SecurityConfig {
    pub(crate) allowed_hosts: Vec<String>,
    pub(crate) allow_force_refresh: bool,
    pub(crate) max_connections: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_hosts: Vec::new(),
            allow_force_refresh: false,
            max_connections: DEFAULT_MAX_CONNECTIONS,
        }
    }
}

impl SecurityConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if self.max_connections == 0 {
            bail!("security.max_connections must be greater than zero");
        }
        Ok(())
    }
}
