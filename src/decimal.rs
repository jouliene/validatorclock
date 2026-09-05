//! Numbers as the chains report them - decimal text - and the arithmetic the
//! round statistics do with it.

/// A year, for turning one round's reward into a yearly rate.
const SECONDS_PER_YEAR: f64 = 365.0 * 24.0 * 60.0 * 60.0;

pub(crate) fn parse_decimal(value: &str) -> Option<f64> {
    let parsed = value.replace(',', "").parse::<f64>().ok()?;
    parsed.is_finite().then_some(parsed)
}

/// The smallest and the largest of a set of decimal strings, given back as the
/// strings themselves.
///
/// The text is what the API serves. Comparing takes numbers, but reporting the
/// number back would round it to whatever an `f64` can hold, and these are
/// token amounts with more digits than that.
pub(crate) fn min_max_decimals<'a>(
    values: impl Iterator<Item = &'a String>,
) -> (Option<String>, Option<String>) {
    let mut parsed = values.filter_map(|value| parse_decimal(value).map(|number| (number, value)));
    let Some(first) = parsed.next() else {
        return (None, None);
    };

    let (min, max) = parsed.fold((first, first), |(min, max), value| {
        let min = if value.0.total_cmp(&min.0).is_lt() {
            value
        } else {
            min
        };
        let max = if value.0.total_cmp(&max.0).is_gt() {
            value
        } else {
            max
        };
        (min, max)
    });

    (Some(min.1.clone()), Some(max.1.clone()))
}

/// What a round's reward comes to as a yearly percentage of what was staked.
///
/// Rounds of the two colours run alongside each other, each covering the whole
/// chain, so a year holds twice as many of them as its length suggests. That is
/// the two in the denominator.
///
/// Nothing is reported for a round that staked nothing, was paid a negative
/// reward, or has no length: each of those is a broken record rather than a
/// return of zero.
pub(crate) fn annual_reward_percent(stake: f64, reward: f64, round_seconds: f64) -> Option<f64> {
    if stake <= 0.0 || reward < 0.0 || round_seconds <= 0.0 {
        return None;
    }

    Some(reward / stake * (SECONDS_PER_YEAR / (round_seconds * 2.0)) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_grouped_decimal_values() {
        assert_eq!(parse_decimal("123.45"), Some(123.45));
        assert_eq!(parse_decimal("1,234.5"), Some(1234.5));
    }

    #[test]
    fn rejects_non_finite_or_invalid_values() {
        assert_eq!(parse_decimal("NaN"), None);
        assert_eq!(parse_decimal("inf"), None);
        assert_eq!(parse_decimal("not-a-number"), None);
    }

    #[test]
    fn the_extremes_come_back_as_the_text_they_arrived_as() {
        let values = [
            "1000.5".to_owned(),
            "999999999999999999999.000000001".to_owned(),
            "12".to_owned(),
        ];
        let (min, max) = min_max_decimals(values.iter());

        assert_eq!(min.as_deref(), Some("12"));
        assert_eq!(
            max.as_deref(),
            Some("999999999999999999999.000000001"),
            "the digits an f64 cannot hold are still the ones served"
        );
    }

    #[test]
    fn text_that_is_not_a_number_takes_part_in_nothing() {
        let values = ["not a number".to_owned(), "7".to_owned()];
        assert_eq!(
            min_max_decimals(values.iter()),
            (Some("7".to_owned()), Some("7".to_owned()))
        );

        let none: [String; 0] = [];
        assert_eq!(min_max_decimals(none.iter()), (None, None));
    }

    #[test]
    fn a_round_that_could_not_have_paid_a_rate_reports_none() {
        assert!(annual_reward_percent(0.0, 1.0, 65_536.0).is_none());
        assert!(annual_reward_percent(-1.0, 1.0, 65_536.0).is_none());
        assert!(annual_reward_percent(100.0, -1.0, 65_536.0).is_none());
        assert!(annual_reward_percent(100.0, 1.0, 0.0).is_none());
    }

    /// Two colours of round run at once, so the yearly figure counts twice as
    /// many rounds as fit end to end in a year.
    #[test]
    fn a_year_holds_two_rounds_for_every_round_length() {
        let round = SECONDS_PER_YEAR / 100.0;
        let percent = annual_reward_percent(100.0, 1.0, round).unwrap();

        assert!((percent - 50.0).abs() < 1e-9, "got {percent}");
    }
}
