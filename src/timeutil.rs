use anyhow::{Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const SECONDS_PER_DAY: u64 = 86_400;

/// Wall-clock seconds since the UNIX epoch, treating a clock set before the
/// epoch as zero. Callers that must report such a clock use [`now_sec_checked`].
pub(crate) fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn now_sec_checked() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX epoch")?
        .as_secs())
}

pub(crate) fn day_index(seconds: u64) -> i64 {
    (seconds / SECONDS_PER_DAY) as i64
}

pub(crate) fn day_string(day_index: i64) -> String {
    let (year, month, day) = civil_from_days(day_index);
    format!("{year:04}-{month:02}-{day:02}")
}

pub(crate) fn parse_day_index(value: &str) -> Option<i64> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn civil_from_days(day_index: i64) -> (i32, u32, u32) {
    let z = day_index + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

fn days_from_civil(mut year: i32, month: u32, day: u32) -> i64 {
    year -= i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let adjusted_month = month as i32 + if month > 2 { -3 } else { 9 };
    let doy = (153 * adjusted_month + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era) * 146_097 + i64::from(doe) - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_conversion_uses_utc_epoch_days() {
        assert_eq!(day_string(0), "1970-01-01");
        assert_eq!(parse_day_index("1970-01-01"), Some(0));
        assert_eq!(
            parse_day_index("2026-06-29").map(day_string),
            Some("2026-06-29".to_owned())
        );
    }

    #[test]
    fn day_index_counts_whole_utc_days() {
        assert_eq!(day_index(0), 0);
        assert_eq!(day_index(SECONDS_PER_DAY - 1), 0);
        assert_eq!(day_index(SECONDS_PER_DAY), 1);
        assert_eq!(day_string(day_index(1_788_000_000)), "2026-08-29");
    }

    #[test]
    fn malformed_days_are_rejected() {
        assert_eq!(parse_day_index("2026-13-01"), None);
        assert_eq!(parse_day_index("2026-01-32"), None);
        assert_eq!(parse_day_index("2026-01-01-01"), None);
        assert_eq!(parse_day_index("not-a-day"), None);
    }

    #[test]
    fn now_is_consistent_across_both_readings() {
        let checked = now_sec_checked().unwrap();
        let unchecked = now_sec();

        assert!(unchecked >= checked);
        assert!(unchecked - checked <= 1);
    }
}
