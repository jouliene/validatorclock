use super::super::dto::{ValidatorElectionHistory, ValidatorRoundData};
use super::super::util::{hex_lower, masterchain_hash_address};
use anyhow::{Context, Result, bail};
use minik2::{Config, CurrentElectionData, Elector, FpTokens, HashBytes, Ref, Transport};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use tycho_types::abi::{AbiType, AbiValue, AbiVersion, FromAbi, WithAbiType};
use tycho_types::cell::Cell;
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

pub(super) async fn fetch_frozen_validator_round_data(
    transport: &Transport,
    config: &Config,
) -> Result<HashMap<u32, ValidatorRoundData>> {
    let data = fetch_full_elector_data(transport, config).await?;
    Ok(frozen_validator_round_data_from_full_elector_data(&data))
}

fn frozen_validator_round_data_from_full_elector_data(
    data: &FullElectorData,
) -> HashMap<u32, ValidatorRoundData> {
    data.past_elections
        .iter()
        .map(|(stake_at, election)| (*stake_at, validator_round_data_from_frozen(election)))
        .collect()
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

    parse_full_elector_data(data)
}

fn parse_full_elector_data(data: &Cell) -> Result<FullElectorData> {
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
