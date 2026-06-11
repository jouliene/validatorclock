use super::*;
use crate::chain::{RoundColor, ValidatorDto, ValidatorMapNodeDto, ValidatorSetDto};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

mod participation;
mod retention;
mod storage;
mod store;
mod window;

fn validator(public_key: &str) -> ValidatorDto {
    validator_with_wallet(public_key, None)
}

fn validator_with_wallet(public_key: &str, wallet: Option<&str>) -> ValidatorDto {
    ValidatorDto {
        public_key: public_key.to_owned(),
        adnl_addr: None,
        wallet: wallet.map(str::to_owned),
        map_node: None,
        last_known_map_node: None,
        source: None,
        contract_type: None,
        contract_type_hash: None,
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
        recent_absent_validators: Vec::new(),
        fake_validator_peers: Vec::new(),
        fake_validator_status_known: false,
    }
}

fn stored_round(set: &ValidatorSetDto, observed_at: u64, complete: bool) -> StoredRound {
    StoredRound {
        round_id: set.round_id,
        round_color: set.round_color,
        utime_since: set.utime_since,
        utime_until: set.utime_until,
        observed_at,
        total_stake: None,
        total_reward: None,
        min_stake: None,
        max_stake: None,
        complete,
        validators: set
            .validators
            .iter()
            .map(|validator| {
                (
                    validator.public_key.clone(),
                    StoredValidator {
                        wallet: validator.wallet.clone(),
                        map_node: validator.map_node.clone(),
                        fake_node: None,
                    },
                )
            })
            .collect(),
    }
}

fn map_node(ip: &str, isp: &str, city: &str, country: &str) -> ValidatorMapNodeDto {
    ValidatorMapNodeDto {
        ip: Some(ip.to_owned()),
        isp: Some(isp.to_owned()),
        city: Some(city.to_owned()),
        country: Some(country.to_owned()),
    }
}

fn record_rounds(store: &mut RoundHistoryStore, chain_id: &str, rounds: &[u32]) {
    let chain = store.chains.entry(chain_id.to_owned()).or_default();
    for round_id in rounds {
        let color = if round_id.is_multiple_of(2) {
            RoundColor::Blue
        } else {
            RoundColor::Green
        };
        chain.record_set(&set(*round_id, color, vec!["alice"]), 100);
    }
}

fn temp_history_path(test_name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "validators_clock_{test_name}_{}_{}.json",
        std::process::id(),
        unique
    ))
}
