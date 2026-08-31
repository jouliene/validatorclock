use serde::Deserialize;
use std::net::IpAddr;
use std::time::Duration;
use tracing::warn;

const BATCH_SIZE: usize = 100;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);

/// One address as ip-api answered for it. `resolved` is false when the service
/// could not place the address, which callers report differently: node
/// locations drop it, visitor stats keep the row and label it unknown.
#[derive(Debug, Clone)]
pub(crate) struct IpApiLocation {
    pub(crate) ip: IpAddr,
    pub(crate) resolved: bool,
    pub(crate) country: Option<String>,
    pub(crate) country_code: Option<String>,
    pub(crate) city: Option<String>,
    pub(crate) isp: Option<String>,
    pub(crate) asn: Option<String>,
    pub(crate) as_name: Option<String>,
    pub(crate) lat: Option<f64>,
    pub(crate) lon: Option<f64>,
}

pub(crate) async fn lookup_batch(endpoint: &str, ips: &[IpAddr]) -> Vec<IpApiLocation> {
    let http = crate::http::shared_client();
    let mut located = Vec::new();

    for chunk in ips.chunks(BATCH_SIZE) {
        let requests = chunk.iter().map(IpAddr::to_string).collect::<Vec<_>>();
        let response = match http
            .post(endpoint)
            .timeout(LOOKUP_TIMEOUT)
            .json(&requests)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                warn!(error = ?error, "ip-api batch lookup failed");
                continue;
            }
        };
        if !response.status().is_success() {
            warn!(status = %response.status(), "ip-api batch lookup returned an error");
            continue;
        }
        let raw = match response.json::<Vec<IpApiResponse>>().await {
            Ok(raw) => raw,
            Err(error) => {
                warn!(error = ?error, "failed to decode ip-api batch response");
                continue;
            }
        };
        located.extend(raw.into_iter().filter_map(IpApiResponse::into_location));
    }

    located
}

pub(crate) fn trimmed_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// ip-api reports autonomous systems as `AS16276 OVH SAS`.
pub(crate) fn parse_asn(value: &str) -> Option<String> {
    let token = value.split_whitespace().next()?;
    token
        .strip_prefix("AS")
        .filter(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
        .map(|_| token.to_owned())
}

pub(crate) fn parse_as_name(value: &str) -> Option<String> {
    let mut parts = value.splitn(2, char::is_whitespace);
    let _asn = parts.next()?;
    parts
        .next()
        .and_then(|name| trimmed_non_empty(name.to_owned()))
}

#[derive(Debug, Deserialize)]
struct IpApiResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default, rename = "countryCode")]
    country_code: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    isp: Option<String>,
    #[serde(default, rename = "as")]
    as_text: Option<String>,
}

impl IpApiResponse {
    fn into_location(self) -> Option<IpApiLocation> {
        let ip = self.query?.parse::<IpAddr>().ok()?;
        Some(IpApiLocation {
            ip,
            resolved: self.status.as_deref() == Some("success"),
            country: self.country.and_then(trimmed_non_empty),
            country_code: self.country_code.and_then(trimmed_non_empty),
            city: self.city.and_then(trimmed_non_empty),
            isp: self.isp.and_then(trimmed_non_empty),
            asn: self.as_text.as_deref().and_then(parse_asn),
            as_name: self.as_text.as_deref().and_then(parse_as_name),
            lat: self.lat.filter(|value| value.is_finite()),
            lon: self.lon.filter(|value| value.is_finite()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location_from(raw: serde_json::Value) -> Option<IpApiLocation> {
        serde_json::from_value::<IpApiResponse>(raw)
            .unwrap()
            .into_location()
    }

    #[test]
    fn a_successful_answer_keeps_every_field() {
        let located = location_from(serde_json::json!({
            "status": "success",
            "query": "203.0.113.9",
            "country": "United States",
            "countryCode": "US",
            "city": "San Francisco",
            "lat": 37.77,
            "lon": -122.41,
            "isp": "OVH SAS",
            "as": "AS16276 OVH SAS",
        }))
        .unwrap();

        assert!(located.resolved);
        assert_eq!(located.ip, "203.0.113.9".parse::<IpAddr>().unwrap());
        assert_eq!(located.country.as_deref(), Some("United States"));
        assert_eq!(located.city.as_deref(), Some("San Francisco"));
        assert_eq!(located.isp.as_deref(), Some("OVH SAS"));
        assert_eq!(located.asn.as_deref(), Some("AS16276"));
        assert_eq!(located.as_name.as_deref(), Some("OVH SAS"));
        assert_eq!(located.lat, Some(37.77));
    }

    #[test]
    fn a_failed_answer_is_returned_without_a_location() {
        let located = location_from(serde_json::json!({
            "status": "fail",
            "query": "203.0.113.9",
            "message": "reserved range",
        }))
        .unwrap();

        assert!(!located.resolved);
        assert_eq!(located.country, None);
        assert_eq!(located.lat, None);
    }

    #[test]
    fn answers_without_a_usable_address_are_dropped() {
        assert!(location_from(serde_json::json!({ "status": "success" })).is_none());
        assert!(
            location_from(serde_json::json!({ "status": "success", "query": "not-an-ip" }))
                .is_none()
        );
    }

    #[test]
    fn blank_and_malformed_fields_become_none() {
        let located = location_from(serde_json::json!({
            "status": "success",
            "query": "203.0.113.9",
            "city": "   ",
            "as": "not-an-asn",
            "lat": f64::NAN,
        }))
        .unwrap();

        assert_eq!(located.city, None);
        assert_eq!(located.asn, None);
        assert_eq!(located.lat, None);
    }
}
