use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

mod acme;
mod app;
mod security;
mod tls;

pub(crate) use acme::AcmeConfig;
pub(crate) use app::{AppConfig, ChainConfig, NodeLocationChainConfig};
pub(crate) use security::SecurityConfig;
pub(crate) use tls::TlsConfig;

const DEFAULT_CONFIG: &str = include_str!("../../validatorclock.json");

#[derive(Debug, Clone)]
pub(crate) struct LoadedConfig {
    pub(crate) config: AppConfig,
    pub(crate) source: ConfigSource,
}

#[derive(Debug, Clone)]
pub(crate) enum ConfigSource {
    Explicit(PathBuf),
    LocalDefault(PathBuf),
    EmbeddedDefault,
}

impl ConfigSource {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Explicit(_) => "explicit",
            Self::LocalDefault(_) => "local_default",
            Self::EmbeddedDefault => "embedded_default",
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Self::Explicit(path) | Self::LocalDefault(path) => Some(path),
            Self::EmbeddedDefault => None,
        }
    }
}

pub(crate) fn load_config(path: Option<&Path>) -> Result<LoadedConfig> {
    let (content, source) = match path {
        Some(path) => (
            fs::read_to_string(path)
                .with_context(|| format!("failed to read config {}", path.display()))?,
            ConfigSource::Explicit(path.to_path_buf()),
        ),
        None => {
            let default_path = PathBuf::from("validatorclock.json");
            match fs::read_to_string(&default_path) {
                Ok(content) => (content, ConfigSource::LocalDefault(default_path)),
                Err(_) => (DEFAULT_CONFIG.into(), ConfigSource::EmbeddedDefault),
            }
        }
    };

    let config =
        serde_json::from_str(&content).context("failed to parse validator clock config")?;
    Ok(LoadedConfig { config, source })
}

#[cfg(test)]
mod tests;
