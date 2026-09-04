use std::env;
use tracing_subscriber::EnvFilter;

/// The log targets belonging to the DHT stack the resolver speaks through.
const NETWORK_STACK_TARGETS: [&str; 3] = ["adnl", "adnl_query", "dht"];

pub(crate) fn init() {
    let default_filter = if env::var_os("VALIDATORCLOCK_DEBUG_HISTORY").is_some() {
        "warn,validatorclock=debug"
    } else {
        "warn,validatorclock=info"
    };
    let requested = env::var("RUST_LOG").ok();
    let filter = EnvFilter::new(quieten_network_stack(
        requested.as_deref().unwrap_or(default_filter),
    ));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Keep the DHT stack's running commentary out of the log.
///
/// It reports at warn what is, for it, ordinary business: a peer that did not
/// answer, a channel reset, and the half-dozen lines every resolver built for
/// a second ask prints on its way out. Over one hour of the running
/// deployment that came to 1171 lines of 1837 - two thirds of the journal -
/// and none of them was anything to act on, while the journal is where a
/// warning that does matter has to be visible. So they are kept at error,
/// where a real failure still comes through.
///
/// Unless the filter names one of them: a person who asks for `adnl=debug` is
/// debugging it, and this does not argue with them.
fn quieten_network_stack(filter: &str) -> String {
    let mut quietened = filter.to_owned();
    for target in NETWORK_STACK_TARGETS {
        if !names_target(filter, target) {
            quietened.push(',');
            quietened.push_str(target);
            quietened.push_str("=error");
        }
    }
    quietened
}

fn names_target(filter: &str, target: &str) -> bool {
    filter.split(',').any(|directive| {
        directive
            .split_once('=')
            .is_some_and(|(name, _)| name.trim() == target)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_network_stack_is_quietened_by_default() {
        let filter = quieten_network_stack("warn,validatorclock=info");
        assert_eq!(
            filter,
            "warn,validatorclock=info,adnl=error,adnl_query=error,dht=error"
        );
        assert!(
            EnvFilter::try_new(&filter).is_ok(),
            "the filter has to be one the subscriber accepts: {filter}"
        );
    }

    #[test]
    fn a_target_someone_asked_for_is_left_alone() {
        let filter = quieten_network_stack("warn,validatorclock=info,adnl=debug");
        assert_eq!(
            filter, "warn,validatorclock=info,adnl=debug,adnl_query=error,dht=error",
            "the one being debugged keeps the level it was given, the rest are still quiet"
        );
    }

    /// A bare level, or a target that merely starts with the same letters, is
    /// not somebody asking for that target.
    #[test]
    fn a_target_is_recognised_by_its_whole_name() {
        assert!(names_target("warn,dht=trace", "dht"));
        assert!(names_target("warn, dht = trace", "dht"));
        assert!(!names_target("warn,dht_storage=trace", "dht"));
        assert!(!names_target("warn", "dht"));
        assert!(
            !names_target("adnl", "adnl"),
            "a bare word is a level, not a target directive"
        );
    }
}
