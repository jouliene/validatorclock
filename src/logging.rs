use std::env;
use tracing_subscriber::EnvFilter;

pub(crate) fn init() {
    let default_filter = if env::var_os("VALIDATORCLOCK_DEBUG_HISTORY").is_some() {
        "warn,validatorclock=debug"
    } else {
        "warn,validatorclock=info"
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
