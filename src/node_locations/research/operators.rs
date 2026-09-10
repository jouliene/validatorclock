//! A small reviewed operator adapter, not an ASN-to-city table.
//! Coordinates represent city centres; they are used only after direct low-RTT evidence.
use super::model::{Entry, Measurement, Observation, distance};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Metro {
    pub id: &'static str,
    pub city: &'static str,
    pub country: &'static str,
    pub code: &'static str,
    pub lat: f64,
    pub lon: f64,
}
// IDs from https://lg.latitude.sh/api/devices; city-centre coordinates, not racks.
pub const METROS: &[Metro] = &[
    Metro {
        id: "sydney",
        city: "Sydney",
        country: "Australia",
        code: "AU",
        lat: -33.8688,
        lon: 151.2093,
    },
    Metro {
        id: "new_york",
        city: "New York",
        country: "United States",
        code: "US",
        lat: 40.7128,
        lon: -74.0060,
    },
    Metro {
        id: "los_angeles",
        city: "Los Angeles",
        country: "United States",
        code: "US",
        lat: 34.0522,
        lon: -118.2437,
    },
    Metro {
        id: "ashburn",
        city: "Ashburn",
        country: "United States",
        code: "US",
        lat: 39.0438,
        lon: -77.4874,
    },
    Metro {
        id: "chicago",
        city: "Chicago",
        country: "United States",
        code: "US",
        lat: 41.8781,
        lon: -87.6298,
    },
    Metro {
        id: "dallas",
        city: "Dallas",
        country: "United States",
        code: "US",
        lat: 32.7767,
        lon: -96.7970,
    },
    Metro {
        id: "miami",
        city: "Miami",
        country: "United States",
        code: "US",
        lat: 25.7617,
        lon: -80.1918,
    },
    Metro {
        id: "silicon_valley",
        city: "San Jose",
        country: "United States",
        code: "US",
        lat: 37.3382,
        lon: -121.8863,
    },
    Metro {
        id: "amsterdam",
        city: "Amsterdam",
        country: "Netherlands",
        code: "NL",
        lat: 52.3676,
        lon: 4.9041,
    },
    Metro {
        id: "london_2",
        city: "London",
        country: "United Kingdom",
        code: "GB",
        lat: 51.5074,
        lon: -0.1278,
    },
    Metro {
        id: "frankfurt",
        city: "Frankfurt",
        country: "Germany",
        code: "DE",
        lat: 50.1109,
        lon: 8.6821,
    },
    Metro {
        id: "tokyo",
        city: "Tokyo",
        country: "Japan",
        code: "JP",
        lat: 35.6762,
        lon: 139.6503,
    },
    Metro {
        id: "singapore",
        city: "Singapore",
        country: "Singapore",
        code: "SG",
        lat: 1.3521,
        lon: 103.8198,
    },
    Metro {
        id: "so_paulo",
        city: "Sao Paulo",
        country: "Brazil",
        code: "BR",
        lat: -23.5505,
        lon: -46.6333,
    },
    Metro {
        id: "buenos_aires",
        city: "Buenos Aires",
        country: "Argentina",
        code: "AR",
        lat: -34.6037,
        lon: -58.3816,
    },
    Metro {
        id: "bogota",
        city: "Bogota",
        country: "Colombia",
        code: "CO",
        lat: 4.7110,
        lon: -74.0721,
    },
    Metro {
        id: "mexico_city",
        city: "Mexico City",
        country: "Mexico",
        code: "MX",
        lat: 19.4326,
        lon: -99.1332,
    },
];
impl Metro {
    pub fn observation(&self, base: &Observation, at: u64) -> Observation {
        Observation {
            source: "latitude-ping".into(),
            at,
            city: self.city.into(),
            country: self.country.into(),
            country_code: self.code.into(),
            lat: self.lat,
            lon: self.lon,
            region: String::new(),
            asn: base.asn.clone(),
            isp: base.isp.clone(),
        }
    }
}

/// Probe only provider IPs with a discrepancy or a point far from known provider metros.
/// Never scan all metros or generalize a neighbour's result to the rest of a prefix.
pub fn candidates(entry: &Entry) -> Vec<Metro> {
    let Some(base) = entry.decision.as_ref() else {
        return vec![];
    };
    if !matches!(base.asn.as_deref(), Some("AS396356" | "AS262287")) {
        return vec![];
    }
    let nearest = |point: &Observation| {
        METROS
            .iter()
            .filter(|m| m.code == point.country_code)
            .min_by(|a, b| {
                distance(point, &a.observation(point, 0))
                    .total_cmp(&distance(point, &b.observation(point, 0)))
            })
    };
    let far = nearest(base).is_some_and(|m| distance(base, &m.observation(base, 0)) > 100.0);
    if entry.reasons.is_empty() && !far {
        return vec![];
    }
    let mut result = Vec::new();
    // Prioritise the alternative so a disproved primary need not consume a second ping.
    for point in entry
        .observations
        .get("dbip")
        .into_iter()
        .chain(std::iter::once(base))
    {
        if let Some(metro) = nearest(point)
            && !result.iter().any(|m: &Metro| m.id == metro.id)
        {
            result.push(*metro);
        }
    }
    result.truncate(2);
    result
}

pub fn apply_measurements(ip: &str, entry: &mut Entry, now: u64, max_age: u64) -> bool {
    let Some(base) = entry.decision.clone() else {
        return false;
    };
    let good = entry
        .measurements
        .iter()
        .filter(|m| m.target == ip && now.saturating_sub(m.at) < max_age)
        .filter_map(|m| Some((METROS.iter().find(|v| v.id == m.vantage)?, m.min_ms?, m.at)))
        .filter(|(_, ms, _)| ms.is_finite() && *ms > 0.0 && *ms <= 3.0)
        .collect::<Vec<_>>();
    let Some((metro, _, at)) = good.iter().min_by(|a, b| a.1.total_cmp(&b.1)).copied() else {
        return false;
    };
    let chosen = metro.observation(&base, at);
    // Conflicting low RTTs can indicate anycast, bad labelling, or proxy replies.
    if good
        .iter()
        .any(|(other, _, _)| distance(&chosen, &other.observation(&base, at)) > 100.0)
    {
        entry.confidence = "disputed".into();
        entry
            .reasons
            .push("incompatible low-latency observations".into());
        return false;
    }
    entry.decision = Some(chosen);
    entry.confidence = "measured_metro".into();
    entry.reasons.push(format!(
        "direct target RTT <=3ms from operator {} (approximate metro, not a facility)",
        metro.id
    ));
    true
}

#[derive(Deserialize)]
pub struct PingResponse {
    pub output: String,
    pub cached: bool,
}
pub fn parse_ping(ip: &str, vantage: &str, at: u64, raw: PingResponse) -> Measurement {
    // Cached external results have no usable observation age here, so don't adopt them.
    let min_ms = if raw.cached {
        None
    } else {
        let header_ok = raw
            .output
            .lines()
            .next()
            .is_some_and(|l| l.starts_with(&format!("PING {ip} ")));
        let replies = raw
            .output
            .lines()
            .filter(|l| l.starts_with(&format!("64 bytes from {ip}:")))
            .filter_map(|line| {
                line.split("time=")
                    .nth(1)?
                    .split_whitespace()
                    .next()?
                    .parse::<f64>()
                    .ok()
            })
            .filter(|n| n.is_finite() && *n > 0.0)
            .collect::<Vec<_>>();
        if header_ok && replies.len() >= 3 {
            replies.into_iter().min_by(f64::total_cmp)
        } else {
            None
        }
    };
    Measurement {
        target: ip.into(),
        vantage: vantage.into(),
        at,
        min_ms,
        output: raw.output.chars().take(8192).collect(),
    }
}
