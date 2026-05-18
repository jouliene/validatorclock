use super::super::dto::ValidatorRoundData;
use super::super::toncenter_client::{TonCenterJsonRpcClient, is_toncenter_json_rpc_endpoint};
use super::super::util::{endpoint_label, masterchain_hash_address, now_sec};
use super::super::{ClockSnapshot, ElectionDto, ElectionTimingsDto, ValidatorSetDto};
use super::effective_validator_sets;
use super::toncenter_stack::{
    election_from_participant_list_extended_stack, validator_round_data_from_past_elections_stack,
};
use crate::config::ChainConfig;
use anyhow::{Context, Result, bail};
use minik2::{HashBytes, ValidatorSet};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use tracing::debug;
use tycho_types::boc::BocRepr;
use tycho_types::cell::Load;
use tycho_types::models::config::ElectionTimings;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_toncenter_json_rpc_endpoint() {
        assert!(is_toncenter_endpoint(
            "https://toncenter.com/api/v2/jsonRPC"
        ));
        assert!(!is_toncenter_endpoint("https://jrpc-ton.broxus.com"));
    }
}
