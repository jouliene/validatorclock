use crate::state::AppState;

mod dto;
mod elector;
mod refresh;
mod status;
mod toncenter_client;
mod util;
mod validator_sources;

pub(crate) use dto::{
    CacheEntry, ChainMeta, ChainsResponse, ClockSnapshot, ElectionCandidateDto, ElectionDto,
    ElectionTimingsDto, RoundColor, RuntimeStatusResponse, ValidatorDto, ValidatorSetDto,
    ValidatorSourceDto,
};
pub(crate) use elector::fetch_chain_snapshot;
pub(crate) use refresh::{get_chain_snapshot_cached_first, spawn_background_refresh};
pub(crate) use status::{chains_response, runtime_status};
use validator_sources::apply_cached_validator_contract_type_hashes;

pub(crate) async fn apply_cached_validator_types_to_snapshot(
    state: &AppState,
    chain_id: &str,
    snapshot: &mut ClockSnapshot,
) {
    let Some(chain) = state.config.chain(chain_id) else {
        return;
    };
    apply_cached_validator_contract_type_hashes(state, chain, snapshot).await;
}

#[cfg(test)]
pub(crate) fn test_clock_snapshot(chain_id: &str) -> ClockSnapshot {
    ClockSnapshot {
        chain: ChainMeta {
            id: chain_id.to_owned(),
            name: "Test".to_owned(),
            color: "#38bdf8".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: "example.com".to_owned(),
        },
        selected_endpoint: Some("https://example.com".to_owned()),
        fetched_at: 123,
        global_id: 42,
        seqno: 7,
        params15: ElectionTimingsDto {
            validators_elected_for: 65_536,
            elections_start_before: 32_768,
            elections_end_before: 8_192,
            stake_held_for: 32_768,
        },
        current_set: ValidatorSetDto {
            utime_since: 1000,
            utime_until: 2000,
            round_id: 10,
            round_color: RoundColor::Blue,
            total: 1,
            main: 1,
            total_weight: "1".to_owned(),
            total_stake: Some("100".to_owned()),
            total_reward: Some("1".to_owned()),
            validators: vec![ValidatorDto {
                public_key: "validator-key".to_owned(),
                adnl_addr: Some("adnl".to_owned()),
                wallet: Some("-1:wallet".to_owned()),
                source: None,
                contract_type: Some("EverWallet".to_owned()),
                contract_type_hash: Some(
                    "3ba6528ab2694c118180aa3bd10dd19ff400b909ab4dcf58fc69925b2c7b12a6".to_owned(),
                ),
                stake: Some("100".to_owned()),
                reward: Some("1".to_owned()),
                weight: "1".to_owned(),
                weight_percent: 100.0,
                history: Vec::new(),
            }],
            recent_absent_validators: Vec::new(),
        },
        previous_set: None,
        next_set: None,
        election: ElectionDto::default(),
        warning: None,
    }
}
