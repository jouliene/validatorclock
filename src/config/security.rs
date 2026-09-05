use anyhow::{Result, bail};
use serde::Deserialize;

/// Connections are now kept alive across a page's polls, so each reader holds
/// theirs for as long as they have the page open rather than for one request.
/// A hundred and twenty-eight of those is a couple of dozen readers, and the
/// hundred and twenty-ninth is turned away. The cost of a spare slot is a
/// buffer; the cost of running out is a reader who cannot open the site.
const DEFAULT_MAX_CONNECTIONS: usize = 512;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct SecurityConfig {
    pub(crate) allowed_hosts: Vec<String>,
    pub(crate) allow_force_refresh: bool,
    pub(crate) max_connections: usize,
    pub(crate) stats_auth: StatsAuthConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_hosts: Vec::new(),
            allow_force_refresh: false,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            stats_auth: StatsAuthConfig::default(),
        }
    }
}

impl SecurityConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if self.max_connections == 0 {
            bail!("security.max_connections must be greater than zero");
        }
        self.stats_auth.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct StatsAuthConfig {
    pub(crate) enabled: bool,
    pub(crate) username: String,
    pub(crate) password: Option<String>,
    pub(crate) password_env: String,
}

impl Default for StatsAuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            username: default_stats_auth_username(),
            password: None,
            password_env: default_stats_auth_password_env(),
        }
    }
}

impl StatsAuthConfig {
    fn validate(&self) -> Result<()> {
        if self.username.trim().is_empty() {
            bail!("security.stats_auth.username cannot be empty");
        }
        if self
            .password
            .as_ref()
            .is_some_and(|password| password.trim().is_empty())
        {
            bail!("security.stats_auth.password cannot be empty when set");
        }
        if self.password_env.trim().is_empty() {
            bail!("security.stats_auth.password_env cannot be empty");
        }
        Ok(())
    }

    pub(crate) fn effective_password(&self) -> Option<String> {
        super::from_config_or_env::secret(self.password.as_deref(), &self.password_env)
    }
}

fn default_stats_auth_username() -> String {
    "admin".to_owned()
}

fn default_stats_auth_password_env() -> String {
    "VALIDATORCLOCK_STATS_PASSWORD".to_owned()
}
