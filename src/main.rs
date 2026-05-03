use anyhow::{Result, anyhow, bail, ensure};
use config::load_config;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

mod chain;
mod config;
mod fsutil;
mod history;
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

    if let Some(backfill) = cli.backfill_history {
        let report = chain::backfill_round_history(
            &config,
            &backfill.chain_id,
            backfill.rounds,
            backfill.max_pages,
        )
        .await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
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
    backfill_history: Option<BackfillHistoryCli>,
}

#[derive(Debug)]
struct BackfillHistoryCli {
    chain_id: String,
    rounds: usize,
    max_pages: usize,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);
        let mut config_path = None;
        let mut once = None;
        let mut backfill_history = None;
        let mut rounds = 10;
        let mut max_pages = 300;

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
                "--backfill-history" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--backfill-history requires a chain id"))?;
                    backfill_history = Some(value);
                }
                "--rounds" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--rounds requires a value"))?;
                    rounds = value
                        .parse()
                        .map_err(|_| anyhow!("--rounds must be a positive integer"))?;
                    ensure!(rounds > 0, "--rounds must be greater than zero");
                }
                "--max-pages" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--max-pages requires a value"))?;
                    max_pages = value
                        .parse()
                        .map_err(|_| anyhow!("--max-pages must be a positive integer"))?;
                    ensure!(max_pages > 0, "--max-pages must be greater than zero");
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: validators_clock [--config validators_clock.json] [--once chain_id] [--backfill-history chain_id --rounds 10 --max-pages 300]"
                    );
                    std::process::exit(0);
                }
                value if !value.starts_with('-') && config_path.is_none() => {
                    config_path = Some(PathBuf::from(value));
                }
                other => bail!("unknown argument `{other}`"),
            }
        }

        ensure!(
            once.is_none() || backfill_history.is_none(),
            "--once and --backfill-history cannot be used together"
        );
        let backfill_history = backfill_history.map(|chain_id| BackfillHistoryCli {
            chain_id,
            rounds,
            max_pages,
        });

        Ok(Self {
            config_path,
            once,
            backfill_history,
        })
    }
}
