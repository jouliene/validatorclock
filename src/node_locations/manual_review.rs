//! Node addresses whose sources disagree, written out for a human to resolve.

use super::fields::unknown_string;
use super::geo_cache::{CachedGeoLocation, GeoCache};
use super::ipinfo::IpInfoLiteLocation;
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use tracing::warn;

pub(super) fn write_manual_review_files(
    manual_review_dir: &Path,
    manual_resolved_dir: &Path,
    chain_id: &str,
    ips: &[IpAddr],
    geo_cache: &GeoCache,
    manual_resolved: &BTreeMap<IpAddr, ManualResolvedIp>,
    now: u64,
) -> Result<usize> {
    let chain_review_dir = manual_review_dir.join(chain_id);
    let mut active_files = BTreeSet::new();

    for ip in ips {
        if manual_resolved.contains_key(ip) {
            continue;
        }
        let Some(location) = geo_cache.location(*ip) else {
            continue;
        };
        if !location.ipinfo_conflict {
            continue;
        }
        let Some(ipinfo) = &location.ipinfo else {
            continue;
        };

        let file_name = manual_ip_file_name(*ip);
        let review_path = chain_review_dir.join(&file_name);
        let manual_path = manual_resolved_dir.join(chain_id).join(&file_name);
        let entry = ManualReviewEntry {
            chain_id: chain_id.to_owned(),
            ip: ip.to_string(),
            detected_at: now,
            reason: location
                .ipinfo_conflict_reason
                .clone()
                .unwrap_or_else(|| "ip-api/ipinfo mismatch".to_owned()),
            ip_api: ReviewIpApiLocation::from(location),
            ipinfo: ipinfo.clone(),
            manual_resolved_path: manual_path.display().to_string(),
        };
        let data =
            serde_json::to_vec_pretty(&entry).context("failed to serialize manual review entry")?;
        write_file_atomic(&review_path, &data, 0o644)?;
        active_files.insert(file_name);
    }

    remove_stale_manual_review_files(&chain_review_dir, &active_files)?;
    Ok(active_files.len())
}

pub(super) fn remove_stale_manual_review_files(
    dir: &Path,
    active_files: &BTreeSet<String>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(file_name) = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if !active_files.contains(&file_name) {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove stale {}", path.display()))?;
        }
    }
    Ok(())
}

pub(super) fn load_manual_resolved_locations(
    manual_resolved_dir: &Path,
    chain_id: &str,
) -> BTreeMap<IpAddr, ManualResolvedIp> {
    let chain_dir = manual_resolved_dir.join(chain_id);
    let mut output = BTreeMap::new();
    let Ok(entries) = fs::read_dir(&chain_dir) else {
        return output;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let body = match fs::read_to_string(&path) {
            Ok(body) => body,
            Err(error) => {
                warn!(path = %path.display(), error = ?error, "failed to read manual resolved IP");
                continue;
            }
        };
        let manual = match serde_json::from_str::<ManualResolvedIp>(&body) {
            Ok(manual) => manual,
            Err(error) => {
                warn!(path = %path.display(), error = ?error, "failed to parse manual resolved IP");
                continue;
            }
        };
        if !manual.geo.latitude.is_finite() || !manual.geo.longitude.is_finite() {
            warn!(path = %path.display(), ip = %manual.ip, "manual resolved IP has invalid coordinates");
            continue;
        }
        output.insert(manual.ip, manual);
    }
    output
}
#[derive(Clone, Debug, Deserialize)]
pub(super) struct ManualResolvedIp {
    pub(super) ip: IpAddr,
    pub(super) geo: ManualGeo,
    #[serde(default, rename = "as")]
    pub(super) as_info: Option<ManualAs>,
    #[serde(default)]
    pub(super) updated_at: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct ManualGeo {
    #[serde(default = "unknown_string")]
    pub(super) city: String,
    #[serde(default = "unknown_string")]
    pub(super) country: String,
    pub(super) latitude: f64,
    pub(super) longitude: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct ManualAs {
    #[serde(default = "unknown_string")]
    pub(super) name: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ManualReviewEntry {
    pub(super) chain_id: String,
    pub(super) ip: String,
    pub(super) detected_at: u64,
    pub(super) reason: String,
    pub(super) ip_api: ReviewIpApiLocation,
    pub(super) ipinfo: IpInfoLiteLocation,
    pub(super) manual_resolved_path: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ReviewIpApiLocation {
    pub(super) city: String,
    pub(super) country: String,
    pub(super) country_code: Option<String>,
    pub(super) isp: String,
    pub(super) asn: Option<String>,
    pub(super) as_name: Option<String>,
    pub(super) latitude: f64,
    pub(super) longitude: f64,
    pub(super) updated_at: u64,
}

impl From<&CachedGeoLocation> for ReviewIpApiLocation {
    fn from(location: &CachedGeoLocation) -> Self {
        Self {
            city: location.city.clone(),
            country: location.country.clone(),
            country_code: location.country_code.clone(),
            isp: location.isp.clone(),
            asn: location.asn.clone(),
            as_name: location.as_name.clone(),
            latitude: location.lat,
            longitude: location.lon,
            updated_at: location.updated_at,
        }
    }
}
pub(super) fn manual_ip_file_name(ip: IpAddr) -> String {
    let safe_ip = ip
        .to_string()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{safe_ip}.json")
}
