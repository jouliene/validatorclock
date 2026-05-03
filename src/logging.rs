use std::env;
use tracing_subscriber::EnvFilter;

pub(crate) fn init() {
    let default_filter = if env::var_os("VALIDATORS_CLOCK_DEBUG_HISTORY").is_some() {
        "warn,validators_clock=debug"
    } else {
        "warn,validators_clock=info"
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
