use super::super::dto::{ValidatorElectionHistory, ValidatorRoundData};
use super::super::toncenter_client::{TonCenterJsonRpcClient, is_toncenter_json_rpc_endpoint};
use super::super::util::{endpoint_label, hex_lower, masterchain_hash_address, now_sec};
use super::super::{
    ClockSnapshot, ElectionCandidateDto, ElectionDto, ElectionTimingsDto, ValidatorSetDto,
};
use super::effective_validator_sets;
use crate::config::ChainConfig;
use anyhow::{Context, Result, bail};
use minik2::{FpTokens, HashBytes, ValidatorSet};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use tracing::debug;
use tycho_types::boc::{Boc, BocRepr};
use tycho_types::cell::{Cell, Load};
use tycho_types::dict;
use tycho_types::models::config::ElectionTimings;
use tycho_types::num::Tokens;

const TON_MAINNET_GLOBAL_ID: i32 = -239;

pub(super) fn is_toncenter_endpoint(endpoint: &str) -> bool {
    is_toncenter_json_rpc_endpoint(endpoint)
}

pub(super) async fn fetch_chain_snapshot(
    chain: &ChainConfig,
    endpoint: &str,
    primary_error: &str,
) -> Result<ClockSnapshot> {
    if chain.id != "ton" {
        bail!("TON Center fallback supports only the `ton` chain");
    }

    let client = TonCenterClient::new(endpoint)?;
    let masterchain = client.get_masterchain_info().await?;
    let timings: ElectionTimings = client
        .get_config_param(15)
        .await?
        .context("TON Center config has no param 15")?;
    let current_validator_set: ValidatorSet = client
        .get_config_param(34)
        .await?
        .context("TON Center config has no param 34")?;
    let next_validator_set = client.get_config_param(36).await?;
    let observed_at = now_sec()?;
    let (current_set, next_set) =
        effective_validator_sets(current_validator_set, next_validator_set, observed_at);

    let elector_address: HashBytes = client
        .get_config_param(1)
        .await?
        .context("TON Center config has no param 1")?;
    let elector_address = masterchain_hash_address(&elector_address.0);
    let election = match client.get_current_election(&elector_address).await {
        Ok(election) => election,
        Err(error) => {
            debug!(
                chain_id = %chain.id,
                error = ?error,
                "failed to fetch TON Center elector participant list"
            );
            ElectionDto::default()
        }
    };
    let validator_round_data = match client.get_past_election_round_data(&elector_address).await {
        Ok(round_data) => round_data,
        Err(error) => {
            debug!(
                chain_id = %chain.id,
                error = ?error,
                "failed to fetch TON Center elector frozen round data"
            );
            HashMap::new()
        }
    };

    Ok(ClockSnapshot {
        chain: super::snapshot::chain_meta_with_rpc(chain, endpoint),
        selected_endpoint: Some(endpoint.to_owned()),
        fetched_at: observed_at,
        global_id: TON_MAINNET_GLOBAL_ID,
        seqno: masterchain.last.seqno,
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
        previous_set: super::snapshot::previous_validator_set(
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
        warning: Some(format!(
            "using TON Center fallback `{}`; primary RPC `{}` failed: {}",
            endpoint_label(endpoint),
            endpoint_label(&chain.rpc),
            primary_error
        )),
    })
}

#[derive(Debug, Clone)]
struct TonCenterClient {
    client: TonCenterJsonRpcClient,
}

impl TonCenterClient {
    fn new(endpoint: &str) -> Result<Self> {
        Ok(Self {
            client: TonCenterJsonRpcClient::new(endpoint)?,
        })
    }

    async fn get_masterchain_info(&self) -> Result<TonCenterMasterchainInfo> {
        self.call("getMasterchainInfo", json!({})).await
    }

    async fn get_config_param<T>(&self, config_id: i32) -> Result<Option<T>>
    where
        for<'a> T: Load<'a>,
    {
        let response: TonCenterConfigInfo = self
            .call("getConfigParam", json!({ "config_id": config_id }))
            .await?;
        let Some(config) = response.config else {
            return Ok(None);
        };
        if config.bytes.is_empty() {
            return Ok(None);
        }

        BocRepr::decode_base64(&config.bytes)
            .map(Some)
            .with_context(|| format!("failed to decode TON Center config param {config_id}"))
    }

    async fn get_current_election(&self, address: &str) -> Result<ElectionDto> {
        let stack = self
            .run_get_method(address, "participant_list_extended")
            .await?;
        election_from_participant_list_extended_stack(&stack)
    }

    async fn get_past_election_round_data(
        &self,
        address: &str,
    ) -> Result<HashMap<u32, ValidatorRoundData>> {
        let stack = self.run_get_method(address, "past_elections").await?;
        validator_round_data_from_past_elections_stack(&stack)
    }

    async fn run_get_method(&self, address: &str, method: &str) -> Result<Vec<Value>> {
        let result: TonCenterRunGetMethodResult = self
            .call(
                "runGetMethod",
                json!({
                    "address": address,
                    "method": method,
                    "stack": [],
                }),
            )
            .await?;
        if result.exit_code != 0 {
            bail!(
                "TON Center `{method}` get-method exited with {}",
                result.exit_code
            );
        }

        Ok(result.stack)
    }

    async fn call<R>(&self, method: &str, params: Value) -> Result<R>
    where
        R: DeserializeOwned,
    {
        self.client.call(method, params).await
    }
}

#[derive(Debug, Deserialize)]
struct TonCenterMasterchainInfo {
    last: TonCenterBlockId,
}

#[derive(Debug, Deserialize)]
struct TonCenterBlockId {
    seqno: u32,
}

#[derive(Debug, Deserialize)]
struct TonCenterConfigInfo {
    config: Option<TonCenterConfigCell>,
}

#[derive(Debug, Deserialize)]
struct TonCenterConfigCell {
    bytes: String,
}

#[derive(Debug, Deserialize)]
struct TonCenterRunGetMethodResult {
    stack: Vec<Value>,
    exit_code: i32,
}

fn election_from_participant_list_extended_stack(stack: &[Value]) -> Result<ElectionDto> {
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

fn validator_round_data_from_past_elections_stack(
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

fn validator_round_data_from_frozen_dict_cell(
    frozen_dict: &Cell,
    total_stake: u128,
    bonuses: u128,
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

fn stack_entry_tuple(entry: &Value) -> Option<&[Value]> {
    if let Some(items) = entry.as_array()
        && items.first()?.as_str()? == "tuple"
    {
        return items.get(1)?.as_array().map(Vec::as_slice);
    }

    let object = entry.as_object()?;
    object
        .get("tuple")
        .and_then(|tuple| {
            tuple
                .get("elements")
                .and_then(Value::as_array)
                .or_else(|| tuple.as_array())
        })
        .map(Vec::as_slice)
}

fn stack_entry_list(entry: &Value) -> Option<&[Value]> {
    if let Some(items) = entry.as_array()
        && items.first()?.as_str()? == "list"
    {
        return items
            .get(1)?
            .get("elements")
            .and_then(Value::as_array)
            .map(Vec::as_slice);
    }

    let object = entry.as_object()?;
    object
        .get("list")
        .and_then(|list| {
            list.get("elements")
                .and_then(Value::as_array)
                .or_else(|| list.as_array())
        })
        .or_else(|| object.get("elements").and_then(Value::as_array))
        .map(Vec::as_slice)
}

fn stack_entry_number_text(entry: &Value) -> Option<&str> {
    if let Some(items) = entry.as_array()
        && items.first()?.as_str()? == "num"
    {
        return items.get(1)?.as_str();
    }

    let object = entry.as_object()?;
    object.get("num").and_then(Value::as_str).or_else(|| {
        object
            .get("number")
            .and_then(|number| number.get("number"))
            .and_then(Value::as_str)
    })
}

fn parse_stack_cell(entry: &Value) -> Result<Cell> {
    let bytes = if let Some(items) = entry.as_array() {
        if items.first().and_then(Value::as_str) == Some("cell") {
            items
                .get(1)
                .and_then(|cell| cell.get("bytes"))
                .and_then(Value::as_str)
        } else {
            None
        }
    } else {
        entry
            .get("cell")
            .and_then(|cell| cell.get("bytes"))
            .and_then(Value::as_str)
    }
    .context("stack entry is not a cell")?;

    Boc::decode_base64(bytes).context("failed to decode TON Center stack cell")
}

fn parse_stack_hash(entry: &Value) -> Result<[u8; 32]> {
    let text = stack_entry_number_text(entry).context("stack entry is not a number")?;
    parse_u256_text(text)
}

fn parse_stack_u32(entry: &Value) -> Result<u32> {
    let value = parse_stack_u128(entry)?;
    u32::try_from(value).context("number does not fit into u32")
}

fn parse_stack_u128(entry: &Value) -> Result<u128> {
    let text = stack_entry_number_text(entry).context("stack entry is not a number")?;
    if let Some(hex) = text.strip_prefix("0x") {
        u128::from_str_radix(hex, 16).context("invalid hex number")
    } else {
        text.parse::<u128>().context("invalid decimal number")
    }
}

fn parse_u256_text(text: &str) -> Result<[u8; 32]> {
    if let Some(hex) = text.strip_prefix("0x") {
        return parse_u256_hex(hex);
    }

    let mut bytes = [0_u8; 32];
    for digit in text.bytes() {
        if !digit.is_ascii_digit() {
            bail!("invalid decimal u256");
        }

        let mut carry = (digit - b'0') as u16;
        for byte in bytes.iter_mut().rev() {
            let value = (*byte as u16) * 10 + carry;
            *byte = value as u8;
            carry = value >> 8;
        }
        if carry != 0 {
            bail!("decimal u256 overflow");
        }
    }

    Ok(bytes)
}

fn parse_u256_hex(hex: &str) -> Result<[u8; 32]> {
    if hex.len() > 64 {
        bail!("hex u256 overflow");
    }

    let mut bytes = [0_u8; 32];
    let mut byte_index = 31_usize;
    let mut low_nibble = true;
    for nibble in hex.bytes().rev() {
        let value = match nibble {
            b'0'..=b'9' => nibble - b'0',
            b'a'..=b'f' => nibble - b'a' + 10,
            b'A'..=b'F' => nibble - b'A' + 10,
            _ => bail!("invalid hex u256"),
        };

        if low_nibble {
            bytes[byte_index] = value;
        } else {
            bytes[byte_index] |= value << 4;
            byte_index = byte_index.saturating_sub(1);
        }
        low_nibble = !low_nibble;
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_toncenter_json_rpc_endpoint() {
        assert!(is_toncenter_endpoint(
            "https://toncenter.com/api/v2/jsonRPC"
        ));
        assert!(!is_toncenter_endpoint("https://jrpc-ton.broxus.com"));
    }

    #[test]
    fn parses_u256_decimal_and_hex() {
        let value = parse_u256_text("256").unwrap();
        assert_eq!(value[30], 1);
        assert_eq!(value[31], 0);

        let value = parse_u256_text("0x0102").unwrap();
        assert_eq!(value[30], 1);
        assert_eq!(value[31], 2);
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
