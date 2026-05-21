use crate::chain::RoundColor;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ParticipationStatus {
    Participated,
    Missed,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ValidatorParticipationDto {
    pub(super) round: u32,
    pub(super) status: ParticipationStatus,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(super) fake_node: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RecentAbsentValidatorDto {
    pub(crate) public_key: String,
    pub(crate) wallet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<RecentAbsentValidatorSourceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) contract_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) contract_type_hash: Option<String>,
    pub(crate) last_seen_round: u32,
    pub(crate) history: Vec<ValidatorParticipationDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RecentAbsentValidatorSourceDto {
    pub(crate) address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) contract_type_hash: Option<String>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RoundHistoryStore {
    #[serde(default)]
    pub(super) chains: HashMap<String, ChainRoundHistory>,
}

impl RoundHistoryStore {
    pub(crate) fn round_count_for_chain(&self, chain_id: &str) -> usize {
        self.chains
            .get(chain_id)
            .map(|chain| chain.rounds.len())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct ChainRoundHistory {
    #[serde(default)]
    pub(super) rounds: BTreeMap<u32, StoredRound>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct StoredRound {
    pub(super) round_id: u32,
    pub(super) round_color: RoundColor,
    pub(super) utime_since: u32,
    pub(super) utime_until: u32,
    pub(super) observed_at: u64,
    #[serde(
        default = "default_complete_history_round",
        skip_serializing_if = "is_complete_history_round"
    )]
    pub(super) complete: bool,
    #[serde(default, deserialize_with = "deserialize_stored_validators")]
    pub(super) validators: BTreeMap<String, StoredValidator>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct StoredValidator {
    pub(super) wallet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) fake_node: Option<bool>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct RoundHistoryDisk {
    pub(super) version: u32,
    #[serde(default)]
    pub(super) chains: HashMap<String, ChainRoundHistory>,
}

fn default_complete_history_round() -> bool {
    true
}

fn is_complete_history_round(complete: &bool) -> bool {
    *complete
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn deserialize_stored_validators<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, StoredValidator>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StoredValidatorsCompat {
        Map(BTreeMap<String, StoredValidator>),
        List(BTreeSet<String>),
    }

    match StoredValidatorsCompat::deserialize(deserializer)? {
        StoredValidatorsCompat::Map(validators) => Ok(validators),
        StoredValidatorsCompat::List(validators) => Ok(validators
            .into_iter()
            .map(|public_key| (public_key, StoredValidator::default()))
            .collect()),
    }
}
