use crate::tls;
use anyhow::{Result, bail};
use instant_acme::LetsEncrypt;
use serde::Deserialize;
use std::path::PathBuf;

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
    pub(super) fn validate(&self) -> Result<()> {
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
