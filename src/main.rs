use anyhow::{Result, anyhow, bail};
use config::load_config;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

mod chain;
mod config;
mod fsutil;
mod history;
mod logging;
mod server;
mod state;
mod tls;
mod validator_types;

use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tls::install_rustls_crypto_provider();

    let cli = Cli::parse()?;
    let loaded_config = load_config(cli.config_path.as_deref())?;
    let config_source = loaded_config.source;
    let config = Arc::new(loaded_config.config);
    config.validate()?;
    if let Some(path) = config_source.path() {
        info!(
            config_source = config_source.label(),
            path = %path.display(),
            "loaded config"
        );
    } else {
        info!(config_source = config_source.label(), "loaded config");
    }

    let state = Arc::new(AppState::new(Arc::clone(&config)));
    chain::spawn_background_refresh(Arc::clone(&state));

    if config.tls.enabled {
        server::run_tls_server(state).await
    } else {
        server::run_plain_http_server(state).await
    }
}

#[derive(Debug)]
struct Cli {
    config_path: Option<PathBuf>,
}

impl Cli {
    fn parse() -> Result<Self> {
        Self::parse_args(env::args().skip(1))
    }

    fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut args = args.into_iter();
        let mut config_path = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--config requires a path"))?;
                    config_path = Some(PathBuf::from(value));
                }
                "--help" | "-h" => {
                    println!("Usage: validators_clock [--config validators_clock.json]");
                    std::process::exit(0);
                }
                value if !value.starts_with('-') && config_path.is_none() => {
                    config_path = Some(PathBuf::from(value));
                }
                other => bail!("unknown argument `{other}`"),
            }
        }

        Ok(Self { config_path })
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use std::path::PathBuf;

    #[test]
    fn cli_parses_explicit_config_path() {
        let cli =
            Cli::parse_args(["--config".to_owned(), "validators_clock.json".to_owned()]).unwrap();

        assert_eq!(
            cli.config_path,
            Some(PathBuf::from("validators_clock.json"))
        );
    }

    #[test]
    fn cli_parses_positional_config_path() {
        let cli = Cli::parse_args(["validators_clock.production.json".to_owned()]).unwrap();

        assert_eq!(
            cli.config_path,
            Some(PathBuf::from("validators_clock.production.json"))
        );
    }

    #[test]
    fn cli_rejects_removed_once_flag() {
        let error = Cli::parse_args(["--once".to_owned(), "everscale".to_owned()])
            .unwrap_err()
            .to_string();

        assert!(error.contains("unknown argument `--once`"));
    }
}
