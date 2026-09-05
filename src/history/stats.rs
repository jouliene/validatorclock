use super::{RoundHistoryStore, StoredRound};
use crate::chain::RoundStatsPointDto;
use crate::decimal::{annual_reward_percent, parse_decimal};

impl RoundHistoryStore {
    pub(crate) fn round_stats_points(&self, chain_id: &str) -> Vec<RoundStatsPointDto> {
        self.chains
            .get(chain_id)
            .map(|chain| {
                chain
                    .rounds
                    .values()
                    .filter_map(StoredRound::round_stats_point)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl StoredRound {
    fn round_stats_point(&self) -> Option<RoundStatsPointDto> {
        if !self.complete || self.validators.is_empty() || self.total_stake.is_none() {
            return None;
        }

        Some(RoundStatsPointDto {
            round_id: self.round_id,
            round_color: self.round_color,
            utime_since: self.utime_since,
            utime_until: self.utime_until,
            validator_count: self.validators.len(),
            total_stake: self.total_stake.clone(),
            total_stake_raw: None,
            min_stake: self.min_stake.clone(),
            max_stake: self.max_stake.clone(),
            total_reward: self.total_reward.clone(),
            total_reward_raw: None,
            profitability_percent: self.profitability_percent(),
        })
    }

    fn profitability_percent(&self) -> Option<f64> {
        let round_seconds = self.utime_until.checked_sub(self.utime_since)?.max(1);
        let stake = self.total_stake.as_deref().and_then(parse_decimal)?;
        let reward = self.total_reward.as_deref().and_then(parse_decimal)?;

        annual_reward_percent(stake, reward, f64::from(round_seconds))
    }
}
