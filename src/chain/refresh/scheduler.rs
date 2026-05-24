use super::get_chain_snapshot;
use crate::state::AppState;
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;
use tokio::time::{Duration, MissedTickBehavior, interval};
use tracing::{info, warn};

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
    let retry_after_seconds = state
        .config
        .refresh_seconds
        .max(10)
        .min(state.config.refresh_timeout_seconds.max(10));
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
    let refresh_seconds = state.config.refresh_seconds.max(10);
    info!(refresh_seconds, "background chain refresh started");
    let mut ticker = interval(Duration::from_secs(refresh_seconds));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        refresh_configured_chains(Arc::clone(&state)).await;
    }
}

async fn refresh_configured_chains(state: Arc<AppState>) {
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
            let task_state = Arc::clone(&state);
            tasks.spawn(async move {
                refresh_chain_and_log(&task_state, &chain_id, RefreshLogKind::Background).await;
            });
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
    let refresh_kind = log_kind.label();
    let started_at = Instant::now();
    match get_chain_snapshot(state, chain_id, true).await {
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
