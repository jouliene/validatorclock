use super::AppState;
use crate::chain::ClockSnapshot;
use crate::config::AppConfig;
use crate::history::{
    RoundHistoryStore, load_round_history_for_chains, round_history_chain_path,
    save_round_history_merged,
};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub(super) fn log_chain_history_paths(config: &AppConfig, round_history_path: &Path) {
    for chain in &config.chains {
        info!(
            chain_id = %chain.id,
            history_path = %round_history_chain_path(round_history_path, &chain.id).display(),
            "chain round history path configured"
        );
    }
}

pub(super) fn load_initial_store(
    config: &AppConfig,
    round_history_path: &Path,
) -> RoundHistoryStore {
    let round_history = load_round_history_for_chains(
        round_history_path,
        config.chains.iter().map(|chain| chain.id.as_str()),
    )
    .unwrap_or_else(|error| {
        warn!(path = %round_history_path.display(), error = ?error, "failed to load round history");
        RoundHistoryStore::default()
    });

    for chain in &config.chains {
        let history_path = round_history_chain_path(round_history_path, &chain.id);
        info!(
            chain_id = %chain.id,
            path = %history_path.display(),
            exists = history_path.exists(),
            rounds = round_history.round_count_for_chain(&chain.id),
            "loaded chain round history"
        );
    }

    round_history
}

impl AppState {
    fn round_history_path_for_chain(&self, chain_id: &str) -> PathBuf {
        round_history_chain_path(&self.round_history_path, chain_id)
    }

    pub(crate) async fn record_round_history(
        &self,
        snapshot: &mut ClockSnapshot,
        observed_at: u64,
    ) {
        let chain_id = snapshot.chain_id().to_owned();
        let retention = RoundHistoryStore::retention_for_snapshot(&chain_id, snapshot);
        let history_path = self.round_history_path_for_chain(&chain_id);
        let history_to_save = {
            let mut history = self.round_history.write().await;
            let rounds_before = history.round_count_for_chain(&chain_id);
            let changed = history.record_snapshot(&chain_id, snapshot, observed_at);
            history.annotate_snapshot(&chain_id, snapshot);
            let rounds_after = history.round_count_for_chain(&chain_id);
            if changed || !history_path.exists() {
                info!(
                    chain_id,
                    path = %history_path.display(),
                    rounds_before,
                    rounds_after,
                    changed,
                    "round history scheduled for save"
                );
            }
            (changed || !history_path.exists()).then(|| history.clone())
        };

        let Some(history_to_save) = history_to_save else {
            return;
        };

        let history_base_path = self.round_history_path.clone();
        let log_history_path = history_path.clone();
        match tokio::task::spawn_blocking(move || {
            save_round_history_merged(&history_base_path, &chain_id, &history_to_save, &retention)
        })
        .await
        {
            Ok(Ok(saved_history)) => {
                self.round_history.write().await.merge_from(saved_history);
            }
            Ok(Err(error)) => {
                warn!(
                    path = %log_history_path.display(),
                    error = ?error,
                    "failed to save round history"
                );
            }
            Err(error) => {
                warn!(
                    path = %log_history_path.display(),
                    error = ?error,
                    "round history save task failed"
                );
            }
        }
    }

    pub(super) async fn annotate_snapshot(&self, chain_id: &str, snapshot: &mut ClockSnapshot) {
        self.round_history
            .read()
            .await
            .annotate_snapshot(chain_id, snapshot);
    }
}
