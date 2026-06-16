use super::AcmeConfig;
use crate::server;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::PathBuf;

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
            cert_path: PathBuf::from("validatorclock_fullchain.pem"),
            key_path: PathBuf::from("validatorclock_privkey.pem"),
            acme: AcmeConfig::default(),
        }
    }
}

impl TlsConfig {
    pub(super) fn validate(&self) -> Result<()> {
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
