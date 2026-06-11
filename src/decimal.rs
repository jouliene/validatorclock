pub(crate) fn parse_decimal(value: &str) -> Option<f64> {
    let parsed = value.replace(',', "").parse::<f64>().ok()?;
    parsed.is_finite().then_some(parsed)
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
}
