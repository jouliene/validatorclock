use crate::{server, tls};
use anyhow::{Context, Result, bail};
use instant_acme::LetsEncrypt;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG: &str = include_str!("../validators_clock.json");
const DEFAULT_MAX_CONNECTIONS: usize = 128;

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

        for chain in &self.chains {
            if chain.id.trim().is_empty() {
                bail!("chain id cannot be empty");
            }
            if chain.name.trim().is_empty() {
                bail!("chain `{}` has empty name", chain.id);
            }
            if chain.rpc.trim().is_empty() {
                bail!("chain `{}` has empty rpc", chain.id);
            }
        }

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
}

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
    fn validate(&self) -> Result<()> {
        if self.max_connections == 0 {
            bail!("security.max_connections must be greater than zero");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct TlsConfig {
    pub(crate) enabled: bool,
    pub(crate) http_listen: String,
    pub(crate) https_listen: String,
    pub(crate) public_url: String,
    pub(crate) cert_path: PathBuf,
    pub(crate) key_path: PathBuf,
    pub(crate) acme: AcmeConfig,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            http_listen: "0.0.0.0:80".to_owned(),
            https_listen: "0.0.0.0:443".to_owned(),
            public_url: String::new(),
            cert_path: PathBuf::from("validators_clock_fullchain.pem"),
            key_path: PathBuf::from("validators_clock_privkey.pem"),
            acme: AcmeConfig::default(),
        }
    }
}

impl TlsConfig {
    fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if !self.public_url.starts_with("https://") {
            bail!("tls.public_url must start with https:// when tls.enabled is true");
        }

        if self.cert_path.as_os_str().is_empty() {
            bail!("tls.cert_path cannot be empty");
        }

        if self.key_path.as_os_str().is_empty() {
            bail!("tls.key_path cannot be empty");
        }

        self.acme.validate()?;
        if self.acme.enabled {
            let public_host = server::public_url_host(&self.public_url)
                .context("tls.public_url must include a host")?;
            let mut public_host_has_certificate = false;
            for identifier in self.acme.identifier_values() {
                let acme_host = server::normalize_host(identifier)
                    .with_context(|| format!("tls.acme identifier `{identifier}` is invalid"))?;
                if public_host == acme_host {
                    public_host_has_certificate = true;
                    break;
                }
            }

            if !public_host_has_certificate {
                bail!(
                    "tls.public_url host `{public_host}` must match tls.acme.identifier or one of tls.acme.extra_identifiers"
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct AcmeConfig {
    pub(crate) enabled: bool,
    pub(crate) staging: bool,
    pub(crate) directory_url: Option<String>,
    pub(crate) identifier: String,
    pub(crate) extra_identifiers: Vec<String>,
    pub(crate) contact: Vec<String>,
    pub(crate) account_path: PathBuf,
    pub(crate) profile: Option<String>,
    pub(crate) renew_after_seconds: Option<u64>,
    pub(crate) retry_timeout_seconds: u64,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            staging: false,
            directory_url: None,
            identifier: String::new(),
            extra_identifiers: Vec::new(),
            contact: Vec::new(),
            account_path: PathBuf::from("validators_clock_acme_account.json"),
            profile: None,
            renew_after_seconds: None,
            retry_timeout_seconds: 60,
        }
    }
}

impl AcmeConfig {
    fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.identifier.trim().is_empty() {
            bail!("tls.acme.identifier cannot be empty when ACME is enabled");
        }

        if self.account_path.as_os_str().is_empty() {
            bail!("tls.acme.account_path cannot be empty");
        }

        if self
            .profile
            .as_deref()
            .is_some_and(|profile| profile.trim().is_empty())
        {
            bail!("tls.acme.profile cannot be empty when set");
        }

        if self.renew_after_seconds == Some(0) {
            bail!("tls.acme.renew_after_seconds must be greater than zero");
        }

        if self.retry_timeout_seconds == 0 {
            bail!("tls.acme.retry_timeout_seconds must be greater than zero");
        }

        tls::acme_identifier(&self.identifier)?;
        for identifier in &self.extra_identifiers {
            if identifier.trim().is_empty() {
                bail!("tls.acme.extra_identifiers cannot contain empty identifiers");
            }
            tls::acme_identifier(identifier)?;
        }
        Ok(())
    }

    pub(crate) fn identifier_values(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.identifier.as_str())
            .chain(self.extra_identifiers.iter().map(String::as_str))
    }

    pub(crate) fn profile_value(&self) -> Option<&str> {
        self.profile
            .as_deref()
            .map(str::trim)
            .filter(|profile| !profile.is_empty())
    }

    pub(crate) fn renew_before_seconds(&self) -> u64 {
        self.renew_after_seconds.unwrap_or_else(|| {
            if self
                .profile_value()
                .is_some_and(|profile| profile.eq_ignore_ascii_case("shortlived"))
            {
                2 * 24 * 60 * 60
            } else {
                30 * 24 * 60 * 60
            }
        })
    }

    pub(crate) fn directory_url(&self) -> String {
        self.directory_url.clone().unwrap_or_else(|| {
            if self.staging {
                LetsEncrypt::Staging.url().to_owned()
            } else {
                LetsEncrypt::Production.url().to_owned()
            }
        })
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

pub(crate) fn load_config(path: Option<&Path>) -> Result<AppConfig> {
    let content = match path {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?,
        None => {
            fs::read_to_string("validators_clock.json").unwrap_or_else(|_| DEFAULT_CONFIG.into())
        }
    };

    serde_json::from_str(&content).context("failed to parse validators clock config")
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
