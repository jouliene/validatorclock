use super::super::dto::{ValidatorElectionHistory, ValidatorRoundData};
use super::super::util::{hex_lower, masterchain_hash_address};
use super::super::{ElectionCandidateDto, ElectionDto};
use anyhow::{Context, Result, bail};
use minik2::FpTokens;
use serde_json::Value;
use std::collections::HashMap;
use tycho_types::cell::{Cell, Load};
use tycho_types::dict;
use tycho_types::num::Tokens;

mod parse;

use parse::{
    parse_stack_cell, parse_stack_hash, parse_stack_u32, parse_stack_u128, stack_entry_list,
    stack_entry_tuple,
};

pub(super) fn election_from_participant_list_extended_stack(
    stack: &[Value],
) -> Result<ElectionDto> {
    let participants = stack
        .get(4)
        .and_then(stack_entry_list)
        .context("TON Center participant_list_extended response has no participant list")?;
    let mut candidates = Vec::with_capacity(participants.len());

    for participant in participants {
        let tuple = stack_entry_tuple(participant)
            .context("TON Center participant entry is not a tuple")?;
        if tuple.len() != 2 {
            bail!(
                "TON Center participant entry has {} fields, expected 2",
                tuple.len()
            );
        }

        let public_key = parse_stack_hash(&tuple[0]).context("invalid participant public key")?;
        let member = stack_entry_tuple(&tuple[1])
            .context("TON Center participant data entry is not a tuple")?;
        if member.len() != 4 {
            bail!(
                "TON Center participant data entry has {} fields, expected 4",
                member.len()
            );
        }

        let stake = parse_stack_u128(&member[0]).context("invalid participant stake")?;
        let stake_factor =
            parse_stack_u32(&member[1]).context("invalid participant stake factor")?;
        let wallet = parse_stack_hash(&member[2]).context("invalid participant wallet")?;
        let adnl_addr = parse_stack_hash(&member[3]).context("invalid participant ADNL address")?;

        candidates.push(ElectionCandidateDto {
            public_key: hex_lower(&public_key),
            stake: FpTokens(stake).to_string(),
            stake_raw: stake.to_string(),
            created_at: 0,
            stake_factor,
            wallet: masterchain_hash_address(&wallet),
            source: None,
            contract_type: None,
            contract_type_hash: None,
            adnl_addr: hex_lower(&adnl_addr),
            history: Vec::new(),
        });
    }

    Ok(ElectionDto { candidates })
}

pub(super) fn validator_round_data_from_past_elections_stack(
    stack: &[Value],
) -> Result<HashMap<u32, ValidatorRoundData>> {
    let elections = stack
        .first()
        .and_then(stack_entry_list)
        .context("TON Center past_elections response has no list")?;
    let mut round_data = HashMap::with_capacity(elections.len());

    for election in elections {
        let tuple =
            stack_entry_tuple(election).context("TON Center past election entry is not a tuple")?;
        let (round_id, frozen_dict_entry, total_stake_entry, bonuses_entry) =
            past_election_data_fields(tuple)?;

        let frozen_dict =
            parse_stack_cell(frozen_dict_entry).context("invalid frozen validator dict")?;
        let total_stake =
            parse_stack_u128(total_stake_entry).context("invalid past election total stake")?;
        let bonuses = parse_stack_u128(bonuses_entry).context("invalid past election bonuses")?;
        let round = validator_round_data_from_frozen_dict_cell(&frozen_dict, total_stake, bonuses)
            .with_context(|| format!("failed to parse frozen validator dict for {round_id}"))?;
        round_data.insert(round_id, round);
    }

    Ok(round_data)
}

fn past_election_data_fields(tuple: &[Value]) -> Result<(u32, &Value, &Value, &Value)> {
    if tuple.len() == 2 {
        let round_id = parse_stack_u32(&tuple[0]).context("invalid past election id")?;
        let data = stack_entry_tuple(&tuple[1])
            .context("TON Center past election data entry is not a tuple")?;
        if data.len() < 6 {
            bail!(
                "TON Center past election data entry has {} fields, expected at least 6",
                data.len()
            );
        }

        return Ok((round_id, &data[3], &data[4], &data[5]));
    }

    if tuple.len() >= 7 {
        let round_id = parse_stack_u32(&tuple[0]).context("invalid past election id")?;
        return Ok((round_id, &tuple[4], &tuple[5], &tuple[6]));
    }

    bail!(
        "TON Center past election entry has {} fields, expected 2 or at least 7",
        tuple.len()
    );
}

/// A validator set is hundreds of entries - the live TON sets hold under 400.
/// A dictionary claiming vastly more is not a validator set, and it can arrive
/// in a few kilobytes: a BOC may reference one cell from many parents, so a
/// short chain of shared cells encodes astronomically many distinct keys. This
/// loop has no await, so no timeout can cut it short, and the release profile
/// aborts rather than unwinds - it has to stop itself.
const MAX_FROZEN_VALIDATORS: usize = 10_000;

fn validator_round_data_from_frozen_dict_cell(
    frozen_dict: &Cell,
    total_stake: u128,
    bonuses: u128,
) -> Result<ValidatorRoundData> {
    frozen_dict_with_limit(frozen_dict, total_stake, bonuses, MAX_FROZEN_VALIDATORS)
}

fn frozen_dict_with_limit(
    frozen_dict: &Cell,
    total_stake: u128,
    bonuses: u128,
    limit: usize,
) -> Result<ValidatorRoundData> {
    struct FrozenValidator {
        public_key: [u8; 32],
        wallet: [u8; 32],
        weight: u64,
        stake: u128,
    }

    let root = dictionary_root_from_stack_cell(frozen_dict)?;
    let mut validators = Vec::new();
    for entry in dict::RawIter::new(&root, 256) {
        if validators.len() >= limit {
            bail!("frozen validator dict has more than {limit} entries; refusing to read further");
        }
        let (key, mut value) = entry.context("invalid frozen validator dict entry")?;
        let public_key = key
            .as_data_slice()
            .load_u256()
            .context("invalid frozen validator public key")?
            .0;
        let wallet = value
            .load_u256()
            .context("invalid frozen validator wallet")?
            .0;
        let weight = value
            .load_u64()
            .context("invalid frozen validator weight")?;
        let stake = Tokens::load_from(&mut value)
            .context("invalid frozen validator stake")?
            .into_inner();

        validators.push(FrozenValidator {
            public_key,
            wallet,
            weight,
            stake,
        });
    }

    let total_weight = validators
        .iter()
        .fold(0_u128, |sum, validator| {
            sum.saturating_add(validator.weight as u128)
        })
        .max(1);
    let validators = validators
        .into_iter()
        .map(|validator| {
            let reward = bonuses.saturating_mul(validator.weight as u128) / total_weight;
            (
                hex_lower(&validator.public_key),
                ValidatorElectionHistory {
                    wallet: masterchain_hash_address(&validator.wallet),
                    stake: FpTokens(validator.stake).to_string(),
                    reward: Some(FpTokens(reward).to_string()),
                    weight: Some(validator.weight.to_string()),
                },
            )
        })
        .collect();

    Ok(ValidatorRoundData {
        validators,
        total_stake: Some(FpTokens(total_stake).to_string()),
        total_stake_raw: Some(total_stake.to_string()),
        total_reward: Some(FpTokens(bonuses).to_string()),
        total_reward_raw: Some(bonuses.to_string()),
        total_weight_raw: Some(total_weight.to_string()),
    })
}

fn dictionary_root_from_stack_cell(cell: &Cell) -> Result<Option<Cell>> {
    let mut slice = cell.as_slice()?;
    if let Ok(root) = Option::<Cell>::load_from(&mut slice)
        && slice.is_data_empty()
        && slice.size_refs() == 0
    {
        return Ok(root);
    }

    Ok(Some(cell.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minik2::HashBytes;
    use serde_json::json;
    use tycho_types::cell::CellBuilder;
    use tycho_types::dict::Dict;

    /// A frozen entry, stored in the value's own bits as the reader expects:
    /// wallet, weight, then the stake as a VarUInt16.
    fn frozen_dict(entries: usize) -> Cell {
        let mut dict = Dict::<HashBytes, (HashBytes, u64, Tokens)>::new();
        for index in 0..entries {
            dict.set(
                HashBytes([index as u8; 32]),
                (HashBytes([index as u8; 32]), 1, Tokens::new(1_000)),
            )
            .unwrap();
        }
        CellBuilder::build_from(dict).unwrap()
    }

    /// A BOC may reference one cell from many parents, so a few kilobytes can
    /// encode astronomically many distinct keys. This loop has no await, so no
    /// timeout can cut it short - it has to stop itself.
    #[test]
    fn a_frozen_dict_larger_than_the_limit_is_refused() {
        let dict = frozen_dict(3);

        assert!(
            frozen_dict_with_limit(&dict, 3_000, 0, 8).is_ok(),
            "an ordinary set should still be read"
        );
        let refused = frozen_dict_with_limit(&dict, 3_000, 0, 2);
        assert!(refused.is_err(), "a dict past the limit should be refused");
        assert!(
            format!("{:?}", refused.unwrap_err()).contains("more than 2 entries"),
            "the error should say why"
        );
    }

    #[test]
    fn locates_nested_past_election_fields() {
        let tuple = vec![
            number("100"),
            json!({
                "tuple": {
                    "elements": [
                        number("101"),
                        number("102"),
                        number("103"),
                        cell("nested-dict"),
                        number("104"),
                        number("105")
                    ]
                }
            }),
        ];

        let (round_id, frozen_dict, total_stake, bonuses) =
            past_election_data_fields(&tuple).unwrap();

        assert_eq!(round_id, 100);
        assert_eq!(cell_bytes(frozen_dict), "nested-dict");
        assert_eq!(parse_stack_u128(total_stake).unwrap(), 104);
        assert_eq!(parse_stack_u128(bonuses).unwrap(), 105);
    }

    #[test]
    fn locates_flattened_past_election_fields() {
        let tuple = vec![
            number("200"),
            number("201"),
            number("202"),
            number("203"),
            cell("flat-dict"),
            number("204"),
            number("205"),
            json!({ "list": { "elements": [] } }),
        ];

        let (round_id, frozen_dict, total_stake, bonuses) =
            past_election_data_fields(&tuple).unwrap();

        assert_eq!(round_id, 200);
        assert_eq!(cell_bytes(frozen_dict), "flat-dict");
        assert_eq!(parse_stack_u128(total_stake).unwrap(), 204);
        assert_eq!(parse_stack_u128(bonuses).unwrap(), 205);
    }

    fn number(value: &str) -> Value {
        json!({
            "number": {
                "number": value
            }
        })
    }

    fn cell(bytes: &str) -> Value {
        json!({
            "cell": {
                "bytes": bytes
            }
        })
    }

    fn cell_bytes(entry: &Value) -> &str {
        entry
            .get("cell")
            .and_then(|cell| cell.get("bytes"))
            .and_then(Value::as_str)
            .unwrap()
    }
}
