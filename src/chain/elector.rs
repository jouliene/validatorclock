use super::dto::{ValidatorElectionHistory, ValidatorRoundData};
use super::util::{endpoint_label, hex_lower, masterchain_hash_address, now_sec, round_color};
use super::{
    ChainMeta, ClockSnapshot, ElectionCandidateDto, ElectionDto, ElectionTimingsDto, ValidatorDto,
    ValidatorSetDto,
};
use crate::config::ChainConfig;
use anyhow::{Context, Result, bail};
use minik2::{
    Config, CurrentElectionData, Elector, FpTokens, HashBytes, Ref, Transport, ValidatorSet,
};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::sync::OnceLock;
use tracing::debug;
use tycho_types::abi::{AbiType, AbiValue, AbiVersion, FromAbi, WithAbiType};
use tycho_types::models::account::AccountState;
use tycho_types::num::Tokens;

#[derive(Debug, Clone, FromAbi, WithAbiType)]
#[allow(dead_code)]
struct FullElectorData {
    current_election: Option<Ref<CurrentElectionData>>,
    credits: BTreeMap<HashBytes, FpTokens>,
    past_elections: BTreeMap<u32, FullPastElectionData>,
    grams: Tokens,
    active_id: u32,
    active_hash: HashBytes,
}

#[derive(Debug, Clone, FromAbi, WithAbiType)]
#[allow(dead_code)]
struct FullPastElectionData {
    unfreeze_at: u32,
    stake_held: u32,
    vset_hash: HashBytes,
    frozen_dict: BTreeMap<HashBytes, FrozenValidator>,
    total_stake: FpTokens,
    bonuses: FpTokens,
}

#[derive(Debug, Clone, FromAbi, WithAbiType)]
struct FrozenValidator {
    addr: HashBytes,
    weight: u64,
    stake: FpTokens,
}

pub(crate) async fn fetch_chain_snapshot(chain: &ChainConfig) -> Result<ClockSnapshot> {
    let transport = Transport::jrpc(&chain.rpc)
        .with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let config = Config::fetch(&transport)
        .await
        .with_context(|| format!("failed to fetch config from `{}`", chain.id))?;
    let timings = config.election_timings()?;
    let current_set = config.current_validator_set()?;
    let next_set = config.next_validator_set()?;
    let election = fetch_election(&transport, &config)
        .await
        .unwrap_or_default();
    // Live refreshes only use elector/full-round state so history can prove both
    // participation and absence for recorded rounds.
    let validator_round_data_result = fetch_frozen_validator_round_data(&transport, &config).await;
    let validator_round_data = match validator_round_data_result {
        Ok(round_data) => round_data,
        Err(error) => {
            if env::var_os("VALIDATORS_CLOCK_DEBUG_HISTORY").is_some() {
                debug!(error = ?error, "validator round data failed");
            }
            HashMap::new()
        }
    };

    Ok(ClockSnapshot {
        chain: ChainMeta::from(chain),
        fetched_at: now_sec()?,
        global_id: config.global_id(),
        seqno: config.seqno(),
        params15: ElectionTimingsDto {
            validators_elected_for: timings.validators_elected_for,
            elections_start_before: timings.elections_start_before,
            elections_end_before: timings.elections_end_before,
            stake_held_for: timings.stake_held_for,
        },
        current_set: ValidatorSetDto::from_set(
            &current_set,
            timings.validators_elected_for,
            validator_round_data.get(&current_set.utime_since),
        ),
        previous_set: previous_validator_set(
            &current_set,
            timings.validators_elected_for,
            &validator_round_data,
        ),
        next_set: next_set.as_ref().map(|set| {
            ValidatorSetDto::from_set(
                set,
                timings.validators_elected_for,
                validator_round_data.get(&set.utime_since),
            )
        }),
        election,
        warning: None,
    })
}

impl From<&ChainConfig> for ChainMeta {
    fn from(chain: &ChainConfig) -> Self {
        Self {
            id: chain.id.clone(),
            name: chain.name.clone(),
            color: chain.color.clone(),
            token_symbol: chain.token_symbol.clone(),
            rpc_label: chain
                .rpc_label
                .clone()
                .unwrap_or_else(|| endpoint_label(&chain.rpc)),
        }
    }
}

impl ValidatorSetDto {
    fn from_set(
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
        })
    }
}

fn previous_validator_set(
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

async fn fetch_election(transport: &Transport, config: &Config) -> Result<ElectionDto> {
    let elector = Elector::from_config(transport, config)?;
    let data = elector.get_data().await?;
    let Some(current) = data.current_election() else {
        return Ok(ElectionDto::default());
    };

    Ok(ElectionDto {
        candidates: current
            .members
            .iter()
            .map(|(public_key, member)| ElectionCandidateDto {
                public_key: hex_lower(&public_key.0),
                stake: member.msg_value.to_string(),
                stake_raw: member.msg_value.0.to_string(),
                created_at: member.created_at,
                stake_factor: member.stake_factor,
                wallet: masterchain_hash_address(&member.src_addr.0),
                source: None,
                contract_type: None,
                contract_type_hash: None,
                adnl_addr: hex_lower(&member.adnl_addr.0),
                history: Vec::new(),
            })
            .collect(),
    })
}

async fn fetch_frozen_validator_round_data(
    transport: &Transport,
    config: &Config,
) -> Result<HashMap<u32, ValidatorRoundData>> {
    let data = fetch_full_elector_data(transport, config).await?;
    Ok(data
        .past_elections
        .iter()
        .map(|(stake_at, election)| (*stake_at, validator_round_data_from_frozen(election)))
        .collect())
}

async fn fetch_full_elector_data(
    transport: &Transport,
    config: &Config,
) -> Result<FullElectorData> {
    let elector = Elector::from_config(transport, config)?;
    let state = transport
        .get_account_state(elector.address().to_string())
        .await?;
    let account = state.account().context("elector account not found")?;

    let AccountState::Active(state_init) = &account.state else {
        bail!("elector account is not active");
    };
    let data = state_init.data.as_ref().context("elector data is empty")?;

    AbiValue::load_partial(
        full_elector_data_abi(),
        AbiVersion::V2_1,
        &mut data.as_slice()?,
    )
    .and_then(FullElectorData::from_abi)
    .context("failed to parse full elector data")
}

fn validator_round_data_from_frozen(election: &FullPastElectionData) -> ValidatorRoundData {
    let total_weight = election
        .frozen_dict
        .values()
        .fold(0_u128, |sum, validator| {
            sum.saturating_add(validator.weight as u128)
        })
        .max(1);
    let validators = election
        .frozen_dict
        .iter()
        .map(|(public_key, validator)| {
            let reward = election.bonuses.0.saturating_mul(validator.weight as u128) / total_weight;
            (
                hex_lower(&public_key.0),
                ValidatorElectionHistory {
                    wallet: masterchain_hash_address(&validator.addr.0),
                    stake: validator.stake.to_string(),
                    reward: Some(FpTokens(reward).to_string()),
                    weight: Some(validator.weight.to_string()),
                },
            )
        })
        .collect();

    ValidatorRoundData {
        validators,
        total_stake: Some(election.total_stake.to_string()),
        total_stake_raw: Some(election.total_stake.0.to_string()),
        total_reward: Some(election.bonuses.to_string()),
        total_reward_raw: Some(election.bonuses.0.to_string()),
        total_weight_raw: Some(total_weight.to_string()),
    }
}

fn full_elector_data_abi() -> &'static AbiType {
    static ABI: OnceLock<AbiType> = OnceLock::new();
    ABI.get_or_init(FullElectorData::abi_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::RoundColor;

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
