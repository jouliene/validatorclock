use super::super::dto::ValidatorRoundData;
use super::super::util::{endpoint_label, hex_lower, round_color};
use super::super::{
    ChainMeta, ClockSnapshot, ElectionDto, ElectionTimingsDto, ValidatorDto, ValidatorSetDto,
};
use crate::config::ChainConfig;
use minik2::{FpTokens, ValidatorSet};
use std::collections::HashMap;
use tycho_types::models::config::ElectionTimings;

/// What one source answered with, before it is a snapshot.
///
/// Three endpoints can serve a chain - a jrpc node, TON Center, a GraphQL
/// gateway - and each fetches these pieces its own way. They are put together
/// in one place so the page cannot come out shaped differently depending on
/// which of them answered.
pub(super) struct SnapshotParts<'a> {
    pub(super) chain: &'a ChainConfig,
    pub(super) endpoint: &'a str,
    pub(super) observed_at: u64,
    pub(super) global_id: i32,
    pub(super) seqno: u32,
    pub(super) timings: ElectionTimings,
    pub(super) current_set: ValidatorSet,
    pub(super) next_set: Option<ValidatorSet>,
    pub(super) election: ElectionDto,
    pub(super) validator_round_data: HashMap<u32, ValidatorRoundData>,
}

pub(super) fn assemble_snapshot(parts: SnapshotParts<'_>) -> ClockSnapshot {
    let SnapshotParts {
        chain,
        endpoint,
        observed_at,
        global_id,
        seqno,
        timings,
        current_set,
        next_set,
        election,
        validator_round_data,
    } = parts;
    let elected_for = timings.validators_elected_for;

    ClockSnapshot {
        chain: chain_meta_with_rpc(chain, endpoint),
        selected_endpoint: Some(endpoint.to_owned()),
        fetched_at: observed_at,
        global_id,
        seqno,
        params15: ElectionTimingsDto {
            validators_elected_for: elected_for,
            elections_start_before: timings.elections_start_before,
            elections_end_before: timings.elections_end_before,
            stake_held_for: timings.stake_held_for,
        },
        current_set: ValidatorSetDto::from_set(
            &current_set,
            elected_for,
            validator_round_data.get(&current_set.utime_since),
        ),
        previous_set: previous_validator_set(&current_set, elected_for, &validator_round_data),
        next_set: next_set.as_ref().map(|set| {
            ValidatorSetDto::from_set(set, elected_for, validator_round_data.get(&set.utime_since))
        }),
        election,
        warning: None,
    }
}

impl From<&ChainConfig> for ChainMeta {
    fn from(chain: &ChainConfig) -> Self {
        chain_meta_with_rpc(chain, &chain.rpc)
    }
}

pub(super) fn chain_meta_with_rpc(chain: &ChainConfig, rpc: &str) -> ChainMeta {
    ChainMeta {
        id: chain.id.clone(),
        name: chain.name.clone(),
        color: chain.color.clone(),
        token_symbol: chain.token_symbol.clone(),
        rpc_label: chain
            .rpc_label
            .clone()
            .unwrap_or_else(|| endpoint_label(rpc)),
    }
}

impl ValidatorSetDto {
    pub(super) fn from_set(
        set: &ValidatorSet,
        validators_elected_for: u32,
        round_data: Option<&ValidatorRoundData>,
    ) -> Self {
        let round_id = set.utime_since / validators_elected_for.max(1);
        let total_weight = set.total_weight.max(1);
        let total_weight_raw = total_weight as u128;
        let total_reward_raw = round_data
            .and_then(|data| data.total_reward_raw.as_deref())
            .and_then(|value| value.parse::<u128>().ok());
        let validator_history = round_data.map(|data| &data.validators);
        Self {
            utime_since: set.utime_since,
            utime_until: set.utime_until,
            round_id,
            round_color: round_color(round_id),
            total: set.list.len(),
            main: set.main.get(),
            total_weight: set.total_weight.to_string(),
            total_stake: round_data.and_then(|data| data.total_stake.clone()),
            total_reward: round_data.and_then(|data| data.total_reward.clone()),
            validators: set
                .list
                .iter()
                .map(|validator| {
                    let public_key = hex_lower(&validator.public_key.0);
                    let history = validator_history.and_then(|history| history.get(&public_key));
                    ValidatorDto {
                        public_key,
                        adnl_addr: validator.adnl_addr.as_ref().map(|adnl| hex_lower(&adnl.0)),
                        wallet: history.map(|history| history.wallet.clone()),
                        map_node: None,
                        last_known_map_node: None,
                        source: None,
                        contract_type: None,
                        contract_type_hash: None,
                        stake: history.map(|history| history.stake.clone()),
                        reward: total_reward_raw
                            .map(|reward| {
                                FpTokens(
                                    reward.saturating_mul(validator.weight as u128)
                                        / total_weight_raw,
                                )
                                .to_string()
                            })
                            .or_else(|| history.and_then(|history| history.reward.clone())),
                        weight: validator.weight.to_string(),
                        weight_percent: validator.weight as f64 * 100.0 / total_weight as f64,
                        history: Vec::new(),
                    }
                })
                .collect(),
            recent_absent_validators: Vec::new(),
            fake_validator_peers: Vec::new(),
            fake_validator_status_known: false,
        }
    }

    fn from_round_data(
        stake_at: u32,
        validators_elected_for: u32,
        round_data: &ValidatorRoundData,
    ) -> Option<Self> {
        if round_data.validators.is_empty() {
            return None;
        }

        let total_weight_raw = round_data
            .total_weight_raw
            .as_deref()
            .and_then(|value| value.parse::<u128>().ok())
            .unwrap_or_else(|| {
                round_data
                    .validators
                    .values()
                    .filter_map(|validator| validator.weight.as_deref())
                    .filter_map(|weight| weight.parse::<u128>().ok())
                    .sum()
            });
        let total_weight = total_weight_raw.max(1);
        let mut validators: Vec<_> = round_data
            .validators
            .iter()
            .map(|(public_key, history)| {
                let weight = history.weight.clone().unwrap_or_else(|| "0".to_owned());
                let weight_raw = weight.parse::<u128>().unwrap_or(0);
                ValidatorDto {
                    public_key: public_key.clone(),
                    adnl_addr: None,
                    wallet: Some(history.wallet.clone()),
                    map_node: None,
                    last_known_map_node: None,
                    source: None,
                    contract_type: None,
                    contract_type_hash: None,
                    stake: Some(history.stake.clone()),
                    reward: history.reward.clone(),
                    weight,
                    weight_percent: weight_raw as f64 * 100.0 / total_weight as f64,
                    history: Vec::new(),
                }
            })
            .collect();
        validators.sort_by(|a, b| {
            b.weight
                .parse::<u128>()
                .unwrap_or(0)
                .cmp(&a.weight.parse::<u128>().unwrap_or(0))
                .then_with(|| a.public_key.cmp(&b.public_key))
        });

        let total = validators.len();
        let round_id = stake_at / validators_elected_for.max(1);
        Some(Self {
            utime_since: stake_at,
            utime_until: stake_at.saturating_add(validators_elected_for),
            round_id,
            round_color: round_color(round_id),
            total,
            main: total.min(u16::MAX as usize) as u16,
            total_weight: total_weight_raw.to_string(),
            total_stake: round_data.total_stake.clone(),
            total_reward: round_data.total_reward.clone(),
            validators,
            recent_absent_validators: Vec::new(),
            fake_validator_peers: Vec::new(),
            fake_validator_status_known: false,
        })
    }
}

pub(super) fn previous_validator_set(
    current_set: &ValidatorSet,
    validators_elected_for: u32,
    validator_round_data: &HashMap<u32, ValidatorRoundData>,
) -> Option<ValidatorSetDto> {
    let previous_stake_at = current_set
        .utime_since
        .checked_sub(validators_elected_for)?;
    let round_data = validator_round_data.get(&previous_stake_at)?;
    ValidatorSetDto::from_round_data(previous_stake_at, validators_elected_for, round_data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::RoundColor;
    use crate::chain::dto::ValidatorElectionHistory;
    use minik2::HashBytes;
    use std::num::NonZeroU16;
    use tycho_types::models::config::ValidatorDescription;

    fn validator_set(utime_since: u32, elected_for: u32, key: u8) -> ValidatorSet {
        ValidatorSet {
            utime_since,
            utime_until: utime_since + elected_for,
            main: NonZeroU16::new(1).unwrap(),
            total_weight: 100,
            list: vec![ValidatorDescription {
                public_key: HashBytes([key; 32]),
                weight: 100,
                adnl_addr: Some(HashBytes([key; 32])),
                mc_seqno_since: 0,
                prev_total_weight: 0,
            }],
        }
    }

    fn round_data(stake: &str) -> ValidatorRoundData {
        let mut validators = HashMap::new();
        validators.insert(
            "aa".to_owned(),
            ValidatorElectionHistory {
                wallet: "-1:aa".to_owned(),
                stake: stake.to_owned(),
                reward: Some("1".to_owned()),
                weight: Some("100".to_owned()),
            },
        );
        ValidatorRoundData {
            validators,
            total_stake: Some(stake.to_owned()),
            total_reward: Some("1".to_owned()),
            total_weight_raw: Some("100".to_owned()),
            ..ValidatorRoundData::default()
        }
    }

    /// Three endpoints can answer for a chain, and each used to put the
    /// snapshot together itself. What the page is made of is stated here now,
    /// once, so it cannot come out different depending on who answered.
    #[test]
    fn a_snapshot_is_the_sets_the_source_answered_with_and_the_round_before_them() {
        let chain = ChainConfig {
            id: "test".to_owned(),
            name: "Test".to_owned(),
            rpc: "https://configured.example".to_owned(),
            rpc_fallbacks: Vec::new(),
            color: "#000000".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: None,
        };
        let elected_for = 100;
        let mut validator_round_data = HashMap::new();
        validator_round_data.insert(100, round_data("10"));

        let snapshot = assemble_snapshot(SnapshotParts {
            chain: &chain,
            endpoint: "https://answered.example",
            observed_at: 1_788_000_000,
            global_id: -239,
            seqno: 42,
            timings: ElectionTimings {
                validators_elected_for: elected_for,
                elections_start_before: 60,
                elections_end_before: 20,
                stake_held_for: 30,
            },
            current_set: validator_set(200, elected_for, 0xaa),
            next_set: Some(validator_set(300, elected_for, 0xbb)),
            election: ElectionDto::default(),
            validator_round_data,
        });

        assert_eq!(snapshot.chain.id, "test");
        assert_eq!(
            snapshot.chain.rpc_label, "answered.example",
            "the page names the endpoint that answered, not the one configured first"
        );
        assert_eq!(
            snapshot.selected_endpoint.as_deref(),
            Some("https://answered.example")
        );
        assert_eq!(snapshot.fetched_at, 1_788_000_000);
        assert_eq!(snapshot.global_id, -239);
        assert_eq!(snapshot.seqno, 42);
        assert_eq!(snapshot.params15.validators_elected_for, elected_for);
        assert_eq!(snapshot.params15.stake_held_for, 30);
        assert_eq!(snapshot.current_set.utime_since, 200);
        assert_eq!(
            snapshot.next_set.expect("the next set is kept").utime_since,
            300
        );
        assert_eq!(
            snapshot
                .previous_set
                .expect("the round before the current one is known from its frozen stakes")
                .utime_since,
            100
        );
        assert!(
            snapshot.warning.is_none(),
            "a snapshot that was fetched has nothing to warn about"
        );
    }

    #[test]
    fn frozen_round_data_builds_previous_validator_set() {
        let mut validators = HashMap::new();
        validators.insert(
            "aa".to_owned(),
            ValidatorElectionHistory {
                wallet: "-1:aa".to_owned(),
                stake: "10".to_owned(),
                reward: Some("1".to_owned()),
                weight: Some("100".to_owned()),
            },
        );
        validators.insert(
            "bb".to_owned(),
            ValidatorElectionHistory {
                wallet: "-1:bb".to_owned(),
                stake: "20".to_owned(),
                reward: Some("2".to_owned()),
                weight: Some("200".to_owned()),
            },
        );
        let round = ValidatorRoundData {
            validators,
            total_stake: Some("30".to_owned()),
            total_reward: Some("3".to_owned()),
            total_weight_raw: Some("300".to_owned()),
            ..ValidatorRoundData::default()
        };

        let set = ValidatorSetDto::from_round_data(200, 100, &round).unwrap();

        assert_eq!(set.round_id, 2);
        assert!(matches!(set.round_color, RoundColor::Blue));
        assert_eq!(set.utime_until, 300);
        assert_eq!(set.total, 2);
        assert_eq!(set.total_weight, "300");
        assert_eq!(set.validators[0].public_key, "bb");
        assert!((set.validators[0].weight_percent - 66.666_666).abs() < 0.001);
    }
}
