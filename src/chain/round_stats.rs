use super::dto::{RoundStatsColorDto, RoundStatsPointDto, ValidatorRoundData};
use super::util::round_color;
use super::{ChainMeta, ChainRoundStatsDto, ClockSnapshot, RoundColor};
use crate::decimal::parse_decimal;
use std::collections::{BTreeMap, HashMap};

const ROUND_STATS_LIMIT_PER_COLOR: usize = 5;
const SECONDS_PER_YEAR: f64 = 365.0 * 24.0 * 60.0 * 60.0;

pub(crate) fn chain_round_stats_from_history(
    snapshot: &ClockSnapshot,
    history_points: Vec<RoundStatsPointDto>,
) -> ChainRoundStatsDto {
    build_round_stats_response(
        snapshot.chain.clone(),
        snapshot.fetched_at,
        snapshot.current_set.utime_since,
        snapshot.params15.validators_elected_for,
        &HashMap::new(),
        &history_points,
    )
}

pub(crate) fn build_round_stats_response(
    chain: ChainMeta,
    fetched_at: u64,
    active_utime_since: u32,
    validators_elected_for: u32,
    validator_round_data: &HashMap<u32, ValidatorRoundData>,
    history_points: &[RoundStatsPointDto],
) -> ChainRoundStatsDto {
    let active_round_id = active_utime_since / validators_elected_for.max(1);
    let mut points_by_round = BTreeMap::new();
    for point in history_points
        .iter()
        .filter(|point| point.utime_since < active_utime_since)
    {
        points_by_round.insert(point.round_id, point.clone());
    }
    for point in validator_round_data
        .iter()
        .filter(|(utime_since, _)| **utime_since < active_utime_since)
        .filter_map(|(utime_since, round_data)| {
            round_stats_point(*utime_since, validators_elected_for, round_data)
        })
    {
        points_by_round.insert(point.round_id, point);
    }
    let points: Vec<_> = points_by_round.into_values().collect();

    ChainRoundStatsDto {
        chain,
        fetched_at,
        active_round_id,
        active_round_color: round_color(active_round_id),
        blue: round_stats_color(RoundColor::Blue, &points),
        green: round_stats_color(RoundColor::Green, &points),
    }
}

fn round_stats_color(color: RoundColor, points: &[RoundStatsPointDto]) -> RoundStatsColorDto {
    let mut rounds: Vec<_> = points
        .iter()
        .filter(|point| point.round_color == color)
        .rev()
        .take(ROUND_STATS_LIMIT_PER_COLOR)
        .cloned()
        .collect();
    rounds.reverse();

    RoundStatsColorDto {
        round_color: color,
        rounds,
    }
}

fn round_stats_point(
    utime_since: u32,
    validators_elected_for: u32,
    round_data: &ValidatorRoundData,
) -> Option<RoundStatsPointDto> {
    if round_data.validators.is_empty() {
        return None;
    }

    let round_id = utime_since / validators_elected_for.max(1);
    let (min_stake, max_stake) = min_max_stakes(round_data);
    let profitability_percent =
        profitability_percent(utime_since, validators_elected_for, round_data);

    Some(RoundStatsPointDto {
        round_id,
        round_color: round_color(round_id),
        utime_since,
        utime_until: utime_since.saturating_add(validators_elected_for),
        validator_count: round_data.validators.len(),
        total_stake: round_data.total_stake.clone(),
        total_stake_raw: round_data.total_stake_raw.clone(),
        min_stake,
        max_stake,
        total_reward: round_data.total_reward.clone(),
        total_reward_raw: round_data.total_reward_raw.clone(),
        profitability_percent,
    })
}

fn min_max_stakes(round_data: &ValidatorRoundData) -> (Option<String>, Option<String>) {
    let mut stakes = round_data.validators.values().filter_map(|validator| {
        parse_decimal(&validator.stake).map(|value| (value, &validator.stake))
    });

    let Some(first) = stakes.next() else {
        return (None, None);
    };

    let (min, max) = stakes.fold((first, first), |(min, max), stake| {
        let min = if stake.0.total_cmp(&min.0).is_lt() {
            stake
        } else {
            min
        };
        let max = if stake.0.total_cmp(&max.0).is_gt() {
            stake
        } else {
            max
        };
        (min, max)
    });

    (Some(min.1.clone()), Some(max.1.clone()))
}

fn profitability_percent(
    utime_since: u32,
    validators_elected_for: u32,
    round_data: &ValidatorRoundData,
) -> Option<f64> {
    let duration = validators_elected_for.max(1) as f64;
    let stake = round_data
        .total_stake_raw
        .as_deref()
        .and_then(parse_decimal)
        .or_else(|| round_data.total_stake.as_deref().and_then(parse_decimal))?;
    let reward = round_data
        .total_reward_raw
        .as_deref()
        .and_then(parse_decimal)
        .or_else(|| round_data.total_reward.as_deref().and_then(parse_decimal))?;

    if stake <= 0.0 || reward < 0.0 || utime_since == 0 {
        return None;
    }

    Some(reward / stake * (SECONDS_PER_YEAR / (duration * 2.0)) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::super::dto::ValidatorElectionHistory;
    use super::*;

    const ROUND_SECONDS: u32 = 65_536;

    #[test]
    fn profitability_annualizes_reward_with_alternating_round_idle_time() {
        let round_data = round_data("10000000", Some("60000"));

        let point = round_stats_point(ROUND_SECONDS, ROUND_SECONDS, &round_data).unwrap();

        let expected =
            60_000.0 / 10_000_000.0 * (SECONDS_PER_YEAR / (f64::from(ROUND_SECONDS) * 2.0)) * 100.0;
        assert!(
            (point.profitability_percent.unwrap() - expected).abs() < 0.000_001,
            "unexpected profitability: {:?}",
            point.profitability_percent
        );
    }

    #[test]
    fn sparse_history_keeps_only_available_completed_rounds() {
        let mut snapshot = crate::chain::test_clock_snapshot("test");
        snapshot.current_set.utime_since = 12 * ROUND_SECONDS;
        snapshot.params15.validators_elected_for = ROUND_SECONDS;
        let history_points = vec![
            history_point(2, Some(2.0)),
            history_point(4, None),
            history_point(8, Some(8.0)),
        ];

        let stats = chain_round_stats_from_history(&snapshot, history_points);

        assert_eq!(
            stats
                .blue
                .rounds
                .iter()
                .map(|round| round.round_id)
                .collect::<Vec<_>>(),
            vec![2, 4, 8]
        );
        assert_eq!(stats.blue.rounds[1].profitability_percent, None);
    }

    #[test]
    fn live_round_data_replaces_history_for_same_round_id() {
        let active_utime_since = 12 * ROUND_SECONDS;
        let history_points = vec![RoundStatsPointDto {
            round_id: 10,
            round_color: RoundColor::Blue,
            utime_since: 10 * ROUND_SECONDS,
            utime_until: 11 * ROUND_SECONDS,
            validator_count: 1,
            total_stake: Some("100".to_owned()),
            total_stake_raw: None,
            min_stake: Some("100".to_owned()),
            max_stake: Some("100".to_owned()),
            total_reward: Some("1".to_owned()),
            total_reward_raw: None,
            profitability_percent: Some(1.0),
        }];
        let live_round_data = HashMap::from([(10 * ROUND_SECONDS, round_data("100", Some("2")))]);

        let stats = build_round_stats_response(
            crate::chain::test_clock_snapshot("test").chain,
            123,
            active_utime_since,
            ROUND_SECONDS,
            &live_round_data,
            &history_points,
        );

        assert_eq!(stats.blue.rounds.len(), 1);
        assert_eq!(stats.blue.rounds[0].round_id, 10);
        assert_eq!(stats.blue.rounds[0].total_reward.as_deref(), Some("2"));
        assert_ne!(stats.blue.rounds[0].profitability_percent, Some(1.0));
    }

    fn history_point(round_id: u32, profitability_percent: Option<f64>) -> RoundStatsPointDto {
        RoundStatsPointDto {
            round_id,
            round_color: RoundColor::Blue,
            utime_since: round_id * ROUND_SECONDS,
            utime_until: (round_id + 1) * ROUND_SECONDS,
            validator_count: 1,
            total_stake: Some("100".to_owned()),
            total_stake_raw: None,
            min_stake: Some("100".to_owned()),
            max_stake: Some("100".to_owned()),
            total_reward: Some("1".to_owned()),
            total_reward_raw: None,
            profitability_percent,
        }
    }

    fn round_data(total_stake: &str, total_reward: Option<&str>) -> ValidatorRoundData {
        ValidatorRoundData {
            validators: HashMap::from([(
                "alice".to_owned(),
                ValidatorElectionHistory {
                    wallet: "-1:alice".to_owned(),
                    stake: total_stake.to_owned(),
                    reward: total_reward.map(str::to_owned),
                    weight: Some("1".to_owned()),
                },
            )]),
            total_stake: Some(total_stake.to_owned()),
            total_reward: total_reward.map(str::to_owned),
            ..Default::default()
        }
    }
}
