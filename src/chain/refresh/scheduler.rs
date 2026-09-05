use super::get_chain_snapshot;
use crate::state::AppState;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;
use tokio::time::{Duration, MissedTickBehavior, interval};
use tracing::{debug, info, warn};

const BACKGROUND_REFRESH_CONCURRENCY: usize = 2;

#[derive(Clone, Copy)]
enum RefreshLogKind {
    Background,
    StaleCache,
}

impl RefreshLogKind {
    fn label(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::StaleCache => "stale_cache",
        }
    }
}

pub(crate) fn spawn_background_refresh(state: Arc<AppState>) {
    tokio::spawn(async move {
        background_refresh_loop(state).await;
    });
}

pub(super) async fn spawn_stale_snapshot_refresh(state: Arc<AppState>, chain_id: String, now: u64) {
    // No sooner than one refresh interval, and no later than the point a
    // refresh already running would have been given up on.
    let retry_after_seconds = state
        .config
        .refresh_seconds
        .min(state.config.refresh_timeout_seconds);
    if !state
        .mark_refresh_attempt_if_due(&chain_id, now, retry_after_seconds)
        .await
    {
        return;
    }

    tokio::spawn(async move {
        refresh_chain_and_log(&state, &chain_id, RefreshLogKind::StaleCache).await;
    });
}

async fn background_refresh_loop(state: Arc<AppState>) {
    let refresh_seconds = state.config.refresh_seconds;
    info!(refresh_seconds, "background chain refresh started");
    let mut ticker = interval(Duration::from_secs(refresh_seconds));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        refresh_configured_chains(Arc::clone(&state)).await;
    }
}

async fn refresh_configured_chains(state: Arc<AppState>) {
    refresh_configured_chains_with(state, |state, chain_id| async move {
        refresh_chain_and_log(&state, &chain_id, RefreshLogKind::Background).await;
    })
    .await;
}

/// A tick refreshes every configured chain, a few at a time.
///
/// Not all at once: a refresh is a handful of calls to somebody else's node,
/// and three chains starting together made the slowest of them slower. Not one
/// at a time either, or a chain whose endpoint is hanging holds up the rest
/// until it gives up.
async fn refresh_configured_chains_with<F, Fut>(state: Arc<AppState>, refresh_one: F)
where
    F: Fn(Arc<AppState>, String) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut chain_ids = state
        .config
        .chains
        .iter()
        .map(|chain| chain.id.clone())
        .collect::<Vec<_>>()
        .into_iter();
    let mut tasks = JoinSet::new();

    loop {
        while tasks.len() < BACKGROUND_REFRESH_CONCURRENCY {
            let Some(chain_id) = chain_ids.next() else {
                break;
            };
            tasks.spawn(refresh_one(Arc::clone(&state), chain_id));
        }

        if tasks.is_empty() {
            break;
        }

        if let Some(result) = tasks.join_next().await
            && let Err(error) = result
        {
            warn!(
                error = ?error,
                "background refresh task failed"
            );
        }
    }
}

async fn refresh_chain_and_log(state: &AppState, chain_id: &str, log_kind: RefreshLogKind) {
    // One refresh of a chain at a time. The timestamp guard above records when
    // a refresh started, so once the retry window passed a second could begin
    // beside one still running - and the background tick never consulted it.
    // Both then finished into the cache, and the slower one won whether or not
    // it had the newer data.
    let Some(_claim) = state.claim_refresh(chain_id) else {
        debug!(chain_id, "a refresh of this chain is already running");
        return;
    };

    let refresh_kind = log_kind.label();
    let started_at = Instant::now();
    // Nobody is waiting on this one, so it may take the whole configured
    // timeout.
    let budget = Duration::from_secs(state.config.refresh_timeout_seconds);
    match get_chain_snapshot(state, chain_id, true, budget).await {
        Ok(snapshot) if snapshot.warning.is_some() => {
            info!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                fetched_at = snapshot.fetched_at,
                round_id = snapshot.current_set.round_id,
                round_color = ?snapshot.current_set.round_color,
                warning = ?snapshot.warning,
                "chain refresh completed with cached data"
            );
        }
        Ok(snapshot) => {
            info!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                fetched_at = snapshot.fetched_at,
                round_id = snapshot.current_set.round_id,
                round_color = ?snapshot.current_set.round_color,
                "chain refresh completed"
            );
        }
        Err(error) => {
            warn!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                error = ?error,
                "chain refresh failed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, ChainConfig};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn chain(id: &str) -> ChainConfig {
        ChainConfig {
            id: id.to_owned(),
            name: id.to_owned(),
            rpc: "https://example.com".to_owned(),
            rpc_fallbacks: Vec::new(),
            color: "#38bdf8".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: None,
        }
    }

    #[derive(Default)]
    struct Watch {
        running: AtomicUsize,
        most_at_once: AtomicUsize,
        refreshed: Mutex<Vec<String>>,
    }

    /// Every chain gets refreshed, and never more than a couple at a time -
    /// the tick is a fan-out to somebody else's nodes, and the whole point of
    /// the limit is that it is neither one at a time nor all at once.
    #[tokio::test(start_paused = true)]
    async fn a_tick_refreshes_every_chain_a_few_at_a_time() {
        let state = Arc::new(AppState::new(Arc::new(AppConfig::for_test(vec![
            chain("one"),
            chain("two"),
            chain("three"),
            chain("four"),
            chain("five"),
        ]))));
        let watch = Arc::new(Watch::default());

        refresh_configured_chains_with(state, |_state, chain_id| {
            let watch = Arc::clone(&watch);
            async move {
                let running = watch.running.fetch_add(1, Ordering::SeqCst) + 1;
                watch.most_at_once.fetch_max(running, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_secs(1)).await;
                watch.refreshed.lock().unwrap().push(chain_id);
                watch.running.fetch_sub(1, Ordering::SeqCst);
            }
        })
        .await;

        let mut refreshed = watch.refreshed.lock().unwrap().clone();
        refreshed.sort();
        assert_eq!(refreshed, ["five", "four", "one", "three", "two"]);
        assert_eq!(
            watch.most_at_once.load(Ordering::SeqCst),
            2,
            "a couple at a time - not one chain at a time, and not all five at once"
        );
    }

    /// The second ask does not go out again while the first still could be
    /// answering: a page open in a browser polls every minute, and a chain
    /// whose endpoint is slow would otherwise collect a refresh per reader.
    #[tokio::test]
    async fn a_stale_page_asks_for_a_refresh_once_per_interval() {
        let state = Arc::new(AppState::new(Arc::new(AppConfig {
            refresh_seconds: 60,
            refresh_timeout_seconds: 90,
            ..AppConfig::for_test(vec![chain("one")])
        })));
        let due = state.config.refresh_seconds;

        assert!(
            state.mark_refresh_attempt_if_due("one", 1_000, due).await,
            "the first reader to find the page stale sets one going"
        );
        assert!(
            !state.mark_refresh_attempt_if_due("one", 1_030, due).await,
            "and the readers behind them do not set another"
        );
        assert!(
            state.mark_refresh_attempt_if_due("one", 1_060, due).await,
            "once the interval is up it may be asked for again"
        );
    }
}
