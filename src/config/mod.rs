use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

mod acme;
mod app;
mod security;
mod tls;

pub(crate) use acme::AcmeConfig;
pub(crate) use app::{AppConfig, ChainConfig};
pub(crate) use security::SecurityConfig;
pub(crate) use tls::TlsConfig;

const DEFAULT_CONFIG: &str = include_str!("../../validators_clock.json");

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
