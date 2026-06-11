use super::{RoundHistoryStore, StoredRound};
use crate::chain::RoundStatsPointDto;

const SECONDS_PER_YEAR: f64 = 365.0 * 24.0 * 60.0 * 60.0;

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
        let duration = self.utime_until.checked_sub(self.utime_since)?.max(1) as f64;
        let stake = self.total_stake.as_deref().and_then(parse_decimal)?;
        let reward = self.total_reward.as_deref().and_then(parse_decimal)?;
        if stake <= 0.0 || reward < 0.0 {
            return None;
        }

        Some(reward / stake * (SECONDS_PER_YEAR / (duration * 2.0)) * 100.0)
    }
}

fn parse_decimal(value: &str) -> Option<f64> {
    let parsed = value.replace(',', "").parse::<f64>().ok()?;
    parsed.is_finite().then_some(parsed)
}
