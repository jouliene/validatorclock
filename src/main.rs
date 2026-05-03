use anyhow::{Result, anyhow, bail};
use config::load_config;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

mod chain;
mod config;
mod fsutil;
mod logging;
mod server;
mod state;
mod tls;

use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tls::install_rustls_crypto_provider();

    let cli = Cli::parse()?;
    let config = Arc::new(load_config(cli.config_path.as_deref())?);
    config.validate()?;

    if let Some(chain_id) = cli.once {
        let chain = config
            .chain(&chain_id)
            .ok_or_else(|| anyhow!("unknown chain id `{chain_id}`"))?;
        let snapshot = chain::fetch_chain_snapshot(chain).await?;
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
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
    once: Option<String>,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);
        let mut config_path = None;
        let mut once = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--config requires a path"))?;
                    config_path = Some(PathBuf::from(value));
                }
                "--once" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--once requires a chain id"))?;
                    once = Some(value);
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: validators_clock [--config validators_clock.json] [--once chain_id]"
                    );
                    std::process::exit(0);
                }
                value if !value.starts_with('-') && config_path.is_none() => {
                    config_path = Some(PathBuf::from(value));
                }
                other => bail!("unknown argument `{other}`"),
            }
        }

        Ok(Self { config_path, once })
    }
}
