use crate::chain::{ClockSnapshot, RoundColor, ValidatorSetDto};
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

const HISTORY_DEPTH: usize = 5;
const MAX_ROUNDS_PER_CHAIN: usize = 100;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ParticipationStatus {
    Participated,
    Missed,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidatorParticipationDto {
    round: u32,
    status: ParticipationStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct RoundHistoryStore {
    #[serde(default)]
    chains: HashMap<String, ChainRoundHistory>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ChainRoundHistory {
    #[serde(default)]
    rounds: BTreeMap<u32, StoredRound>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRound {
    round_id: u32,
    round_color: RoundColor,
    utime_since: u32,
    utime_until: u32,
    observed_at: u64,
    validators: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RoundHistoryDisk {
    version: u32,
    #[serde(default)]
    chains: HashMap<String, ChainRoundHistory>,
}

impl RoundHistoryStore {
    pub(crate) fn record_snapshot(
        &mut self,
        chain_id: &str,
        snapshot: &ClockSnapshot,
        observed_at: u64,
    ) {
        let chain = self.chains.entry(chain_id.to_owned()).or_default();
        chain.record_set(&snapshot.current_set, observed_at);
        if let Some(previous_set) = &snapshot.previous_set {
            chain.record_set(previous_set, observed_at);
        }
        if let Some(next_set) = &snapshot.next_set {
            chain.record_set(next_set, observed_at);
        }
        chain.prune();
    }

    pub(crate) fn annotate_snapshot(&self, chain_id: &str, snapshot: &mut ClockSnapshot) {
        self.annotate_set(chain_id, &mut snapshot.current_set);
        if let Some(previous_set) = &mut snapshot.previous_set {
            self.annotate_set(chain_id, previous_set);
        }
        if let Some(next_set) = &mut snapshot.next_set {
            self.annotate_set(chain_id, next_set);
        }
    }

    fn annotate_set(&self, chain_id: &str, set: &mut ValidatorSetDto) {
        for validator in &mut set.validators {
            validator.history = self.same_color_participation(
                chain_id,
                set.round_id,
                set.round_color,
                &validator.public_key,
            );
        }
    }

    fn same_color_participation(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
        public_key: &str,
    ) -> Vec<ValidatorParticipationDto> {
        let chain = self.chains.get(chain_id);
        (1..=HISTORY_DEPTH)
            .map(|step| {
                let round = round_id.saturating_sub((step * 2) as u32);
                let status = chain
                    .and_then(|chain| chain.rounds.get(&round))
                    .filter(|stored| stored.round_color == round_color)
                    .map(|stored| {
                        if stored.validators.contains(public_key) {
                            ParticipationStatus::Participated
                        } else {
                            ParticipationStatus::Missed
                        }
                    })
                    .unwrap_or(ParticipationStatus::Unknown);
                ValidatorParticipationDto { round, status }
            })
            .collect()
    }
}

impl ChainRoundHistory {
    fn record_set(&mut self, set: &ValidatorSetDto, observed_at: u64) {
        if set.validators.is_empty() {
            return;
        }

        self.rounds.insert(
            set.round_id,
            StoredRound {
                round_id: set.round_id,
                round_color: set.round_color,
                utime_since: set.utime_since,
                utime_until: set.utime_until,
                observed_at,
                validators: set
                    .validators
                    .iter()
                    .map(|validator| validator.public_key.clone())
                    .collect(),
            },
        );
    }

    fn prune(&mut self) {
        while self.rounds.len() > MAX_ROUNDS_PER_CHAIN {
            let Some(oldest) = self.rounds.keys().next().copied() else {
                break;
            };
            self.rounds.remove(&oldest);
        }
    }
}

pub(crate) fn load_round_history(path: &Path) -> Result<RoundHistoryStore> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(RoundHistoryStore::default());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let disk: RoundHistoryDisk = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(RoundHistoryStore {
        chains: disk.chains,
    })
}

pub(crate) fn save_round_history(path: &Path, history: &RoundHistoryStore) -> Result<()> {
    let disk = RoundHistoryDisk {
        version: 1,
        chains: history.chains.clone(),
    };
    let content = serde_json::to_string_pretty(&disk)?;
    write_file_atomic(path, content.as_bytes(), 0o644)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{RoundColor, ValidatorDto};

    fn validator(public_key: &str) -> ValidatorDto {
        ValidatorDto {
            public_key: public_key.to_owned(),
            adnl_addr: None,
            wallet: None,
            stake: None,
            reward: None,
            weight: "1".to_owned(),
            weight_percent: 100.0,
            history: Vec::new(),
        }
    }

    fn set(round_id: u32, round_color: RoundColor, validators: Vec<&str>) -> ValidatorSetDto {
        ValidatorSetDto {
            utime_since: round_id * 10,
            utime_until: round_id * 10 + 10,
            round_id,
            round_color,
            total: validators.len(),
            main: validators.len() as u16,
            total_weight: validators.len().to_string(),
            total_stake: None,
            total_reward: None,
            validators: validators.into_iter().map(validator).collect(),
        }
    }

    #[test]
    fn same_color_participation_marks_known_misses_and_unknowns() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        chain.record_set(&set(8, RoundColor::Blue, vec!["alice"]), 100);
        chain.record_set(&set(6, RoundColor::Blue, vec!["bob"]), 100);

        let history = store.same_color_participation("test", 10, RoundColor::Blue, "alice");
        assert!(matches!(
            history[0].status,
            ParticipationStatus::Participated
        ));
        assert!(matches!(history[1].status, ParticipationStatus::Missed));
        assert!(matches!(history[2].status, ParticipationStatus::Unknown));
    }
}
