//! Small helpers shared by the geo, ipinfo, and map node modules.

use serde_json::Value;

pub(super) fn is_zero(value: &u64) -> bool {
    *value == 0
}
pub(super) fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .and_then(crate::geoip::sanitized_field)
}
pub(super) fn normalized_code(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase)
}
pub(super) fn normalized_name(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "unknown" {
        return None;
    }

    let normalized = normalized
        .strip_prefix("the ")
        .unwrap_or(&normalized)
        .trim();
    Some(match normalized {
        "netherland" | "netherlands" => "netherlands".to_owned(),
        _ => normalized.to_owned(),
    })
}
pub(super) fn is_false(value: &bool) -> bool {
    !*value
}
pub(super) fn number_field(value: &Value, field: &str) -> Option<f64> {
    value
        .get(field)
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
        // "NaN" parses as a number but is not one: it would place a node
        // nowhere and travel all the way to the map.
        .filter(|value| value.is_finite())
}
pub(super) fn number_u64_field(value: &Value, field: &str) -> Option<u64> {
    value
        .get(field)
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
}
pub(super) fn unknown_if_empty(value: &str) -> String {
    crate::geoip::sanitized_field(value).unwrap_or_else(unknown_string)
}
pub(super) fn unknown_string() -> String {
    "Unknown".to_owned()
}
pub(super) fn ip_api_source() -> String {
    "ip-api".to_owned()
}
pub(super) fn medium_confidence() -> String {
    "medium".to_owned()
}
