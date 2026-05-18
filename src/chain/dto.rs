use crate::history::{RecentAbsentValidatorDto, ValidatorParticipationDto};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChainsResponse {
    pub(super) refresh_seconds: u64,
    pub(super) chains: Vec<ChainMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeStatusResponse {
    pub(super) status: &'static str,
    pub(super) version: &'static str,
    pub(super) started_at: Option<u64>,
    pub(super) uptime_seconds: u64,
    pub(super) refresh_seconds: u64,
    pub(super) refresh_timeout_seconds: u64,
    pub(super) chains: Vec<ChainRuntimeStatusDto>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ChainRuntimeStatusDto {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) cached: bool,
    pub(super) fetched_at: Option<u64>,
    pub(super) age_seconds: Option<u64>,
    pub(super) stale: bool,
    pub(super) last_attempt_at: Option<u64>,
    pub(super) last_success_at: Option<u64>,
    pub(super) last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChainMeta {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) color: String,
    pub(super) token_symbol: String,
    pub(super) rpc_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ClockSnapshot {
    pub(crate) chain: ChainMeta,
    #[serde(skip)]
    pub(crate) selected_rpc: Option<String>,
    pub(crate) fetched_at: u64,
    pub(crate) global_id: i32,
    pub(crate) seqno: u32,
    pub(crate) params15: ElectionTimingsDto,
    pub(crate) current_set: ValidatorSetDto,
    pub(crate) previous_set: Option<ValidatorSetDto>,
    pub(crate) next_set: Option<ValidatorSetDto>,
    pub(crate) election: ElectionDto,
    pub(crate) warning: Option<String>,
}

impl ClockSnapshot {
    pub(crate) fn chain_id(&self) -> &str {
        &self.chain.id
    }

    pub(crate) fn fetched_at(&self) -> u64 {
        self.fetched_at
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ElectionTimingsDto {
    pub(super) validators_elected_for: u32,
    pub(super) elections_start_before: u32,
    pub(super) elections_end_before: u32,
    pub(super) stake_held_for: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidatorSetDto {
    pub(crate) utime_since: u32,
    pub(crate) utime_until: u32,
    pub(crate) round_id: u32,
    pub(crate) round_color: RoundColor,
    pub(crate) total: usize,
    pub(crate) main: u16,
    pub(crate) total_weight: String,
    pub(crate) total_stake: Option<String>,
    pub(crate) total_reward: Option<String>,
    pub(crate) validators: Vec<ValidatorDto>,
    pub(crate) recent_absent_validators: Vec<RecentAbsentValidatorDto>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RoundColor {
    Blue,
    Green,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidatorDto {
    pub(crate) public_key: String,
    pub(crate) adnl_addr: Option<String>,
    pub(crate) wallet: Option<String>,
    pub(crate) source: Option<ValidatorSourceDto>,
    pub(crate) contract_type: Option<String>,
    pub(crate) contract_type_hash: Option<String>,
    pub(crate) stake: Option<String>,
    pub(crate) reward: Option<String>,
    pub(crate) weight: String,
    pub(crate) weight_percent: f64,
    pub(crate) history: Vec<ValidatorParticipationDto>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidatorSourceDto {
    pub(crate) address: String,
    pub(crate) contract_type_hash: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ElectionDto {
    pub(crate) candidates: Vec<ElectionCandidateDto>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ElectionCandidateDto {
    pub(crate) public_key: String,
    pub(super) stake: String,
    pub(super) stake_raw: String,
    pub(super) created_at: u32,
    pub(super) stake_factor: u32,
    pub(crate) wallet: String,
    pub(crate) source: Option<ValidatorSourceDto>,
    pub(crate) contract_type: Option<String>,
    pub(crate) contract_type_hash: Option<String>,
    pub(super) adnl_addr: String,
    pub(crate) history: Vec<ValidatorParticipationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ValidatorElectionHistory {
    pub(super) wallet: String,
    pub(super) stake: String,
    #[serde(default)]
    pub(super) reward: Option<String>,
    #[serde(default)]
    pub(super) weight: Option<String>,
}

pub(super) type ValidatorHistory = HashMap<String, ValidatorElectionHistory>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct ValidatorRoundData {
    #[serde(default)]
    pub(super) validators: ValidatorHistory,
    #[serde(default)]
    pub(super) total_stake: Option<String>,
    #[serde(default)]
    pub(super) total_stake_raw: Option<String>,
    #[serde(default)]
    pub(super) total_reward: Option<String>,
    #[serde(default)]
    pub(super) total_reward_raw: Option<String>,
    #[serde(default)]
    pub(super) total_weight_raw: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CacheEntry {
    pub(super) fetched_at: u64,
    pub(super) snapshot: ClockSnapshot,
}

impl CacheEntry {
    pub(crate) fn new(fetched_at: u64, snapshot: ClockSnapshot) -> Self {
        Self {
            fetched_at,
            snapshot,
        }
    }

    pub(crate) fn fetched_at(&self) -> u64 {
        self.fetched_at
    }

    pub(crate) fn snapshot(&self) -> &ClockSnapshot {
        &self.snapshot
    }
}
