use crate::fsutil;
use anyhow::{Context, Result, anyhow};
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

use super::AppState;

type HmacSha256 = Hmac<Sha256>;

const SESSION_TIMEOUT_SECONDS: u64 = 1_800;
const ONLINE_WINDOW_SECONDS: u64 = 120;
const VISITOR_HASH_RETENTION_DAYS: i64 = 35;
const SECONDS_PER_DAY: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnalyticsEventKind {
    PageOpen,
    Heartbeat,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PublicAnalytics {
    today: PublicAnalyticsToday,
    last_7_days: PublicAnalyticsWindow,
    last_30_days: PublicAnalyticsWindow,
    all_time: PublicAnalyticsAllTime,
}

#[derive(Debug, Clone, Serialize)]
struct PublicAnalyticsToday {
    online_now: u64,
    unique_visitors: u64,
    visits: u64,
    pageviews: u64,
}

#[derive(Debug, Clone, Serialize)]
struct PublicAnalyticsWindow {
    unique_visitor_days: u64,
    visits: u64,
    pageviews: u64,
}

#[derive(Debug, Clone, Serialize)]
struct PublicAnalyticsAllTime {
    visits: u64,
    pageviews: u64,
}

#[derive(Debug)]
pub(super) struct AnalyticsRuntime {
    disk: AnalyticsDisk,
    secret: [u8; 32],
    sessions: HashMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AnalyticsDisk {
    #[serde(default)]
    all_time: AnalyticsAllTime,
    #[serde(default)]
    days: BTreeMap<String, AnalyticsDay>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AnalyticsAllTime {
    #[serde(default)]
    pageviews: u64,
    #[serde(default)]
    visits: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AnalyticsDay {
    #[serde(default)]
    pageviews: u64,
    #[serde(default)]
    visits: u64,
    #[serde(default)]
    unique_visitors: u64,
    #[serde(default)]
    visitor_hashes: BTreeSet<String>,
}

pub(super) fn load_initial_runtime(path: &Path) -> AnalyticsRuntime {
    let disk = match load_analytics_disk(path) {
        Ok(disk) => disk,
        Err(error) => {
            warn!(
                path = %path.display(),
                error = ?error,
                "failed to load analytics store; starting with empty analytics state"
            );
            AnalyticsDisk::default()
        }
    };
    let secret_path = analytics_secret_path(path);
    let secret = match load_or_create_secret(&secret_path) {
        Ok(secret) => secret,
        Err(error) => {
            warn!(
                path = %secret_path.display(),
                error = ?error,
                "failed to load analytics secret; using a process-local fallback"
            );
            fallback_process_secret()
        }
    };

    AnalyticsRuntime {
        disk,
        secret,
        sessions: HashMap::new(),
    }
}

impl AppState {
    pub(crate) async fn record_analytics_event(
        &self,
        event: AnalyticsEventKind,
        peer_addr: Option<SocketAddr>,
        headers: &HeaderMap,
    ) {
        if is_bot_request(headers) {
            return;
        }

        let Some(peer_addr) = peer_addr else {
            return;
        };
        let now = now_seconds();
        let today_index = (now / SECONDS_PER_DAY) as i64;
        let today = day_string(today_index);
        let visitor_key = {
            let runtime = self.analytics.lock().await;
            visitor_key(&runtime.secret, &today, peer_addr.ip(), headers)
        };

        let snapshot = {
            let mut runtime = self.analytics.lock().await;
            runtime
                .sessions
                .retain(|_, last_seen| now.saturating_sub(*last_seen) <= SESSION_TIMEOUT_SECONDS);
            prune_visitor_hashes(&mut runtime.disk, today_index);

            let starts_visit = runtime
                .sessions
                .get(&visitor_key)
                .is_none_or(|last_seen| now.saturating_sub(*last_seen) > SESSION_TIMEOUT_SECONDS);
            let counts_pageview = event == AnalyticsEventKind::PageOpen;

            {
                let day = runtime.disk.days.entry(today).or_default();
                if day.visitor_hashes.insert(visitor_key.clone()) {
                    day.unique_visitors = day.unique_visitors.saturating_add(1);
                }
                if starts_visit {
                    day.visits = day.visits.saturating_add(1);
                }
                if counts_pageview {
                    day.pageviews = day.pageviews.saturating_add(1);
                }
            }

            if starts_visit {
                runtime.disk.all_time.visits = runtime.disk.all_time.visits.saturating_add(1);
            }
            if counts_pageview {
                runtime.disk.all_time.pageviews = runtime.disk.all_time.pageviews.saturating_add(1);
            }

            runtime.sessions.insert(visitor_key, now);
            runtime.disk.clone()
        };

        if let Err(error) = save_analytics_disk(&self.analytics_path, &snapshot) {
            warn!(
                path = %self.analytics_path.display(),
                error = ?error,
                "failed to persist analytics store"
            );
        }
    }

    pub(crate) async fn public_analytics(&self) -> PublicAnalytics {
        let now = now_seconds();
        let today_index = (now / SECONDS_PER_DAY) as i64;
        let today_key = day_string(today_index);
        let mut runtime = self.analytics.lock().await;
        runtime
            .sessions
            .retain(|_, last_seen| now.saturating_sub(*last_seen) <= SESSION_TIMEOUT_SECONDS);
        let online_now = runtime
            .sessions
            .values()
            .filter(|last_seen| now.saturating_sub(**last_seen) <= ONLINE_WINDOW_SECONDS)
            .count() as u64;
        let today = runtime
            .disk
            .days
            .get(&today_key)
            .cloned()
            .unwrap_or_default();
        PublicAnalytics {
            today: PublicAnalyticsToday {
                online_now,
                unique_visitors: today.unique_visitors,
                visits: today.visits,
                pageviews: today.pageviews,
            },
            last_7_days: analytics_window(&runtime.disk, today_index, 7),
            last_30_days: analytics_window(&runtime.disk, today_index, 30),
            all_time: PublicAnalyticsAllTime {
                visits: runtime.disk.all_time.visits,
                pageviews: runtime.disk.all_time.pageviews,
            },
        }
    }
}

fn load_analytics_disk(path: &Path) -> Result<AnalyticsDisk> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AnalyticsDisk::default()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn save_analytics_disk(path: &Path, disk: &AnalyticsDisk) -> Result<()> {
    let content = serde_json::to_vec_pretty(disk)?;
    fsutil::write_file_atomic(path, &content, 0o600)
}

fn analytics_secret_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("validatorclock_analytics.json");
    path.with_file_name(format!("{file_name}.secret"))
}

fn load_or_create_secret(path: &Path) -> Result<[u8; 32]> {
    match fs::read_to_string(path) {
        Ok(content) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                    .with_context(|| format!("failed to set permissions on {}", path.display()))?;
            }
            decode_secret(content.trim()).with_context(|| format!("invalid {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let secret = generate_secret()?;
            let content = format!("{}\n", hex::encode(secret));
            fsutil::write_file_atomic(path, content.as_bytes(), 0o600)?;
            info!(path = %path.display(), "created analytics secret");
            Ok(secret)
        }
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn decode_secret(value: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(value)?;
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow!("expected 32 secret bytes, got {}", bytes.len()))
}

fn generate_secret() -> Result<[u8; 32]> {
    let mut secret = [0u8; 32];
    fs::File::open("/dev/urandom")
        .context("failed to open /dev/urandom")?
        .read_exact(&mut secret)
        .context("failed to read /dev/urandom")?;
    Ok(secret)
}

fn fallback_process_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    let seed = now_seconds().to_le_bytes();
    for chunk in secret.chunks_mut(seed.len()) {
        let len = chunk.len();
        chunk.copy_from_slice(&seed[..len]);
    }
    secret
}

fn visitor_key(secret: &[u8; 32], day: &str, ip: IpAddr, headers: &HeaderMap) -> String {
    let ip_prefix = masked_ip_prefix(ip);
    let user_agent_family =
        header_family(headers, axum::http::header::USER_AGENT, user_agent_family);
    let language_family = header_family(
        headers,
        axum::http::header::ACCEPT_LANGUAGE,
        accept_language_family,
    );
    let input = format!("{day}|{ip_prefix}|{user_agent_family}|{language_family}");
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts fixed-size keys");
    mac.update(input.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn masked_ip_prefix(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            format!("{a}.{b}.{c}.0/24")
        }
        IpAddr::V6(ip) => {
            let mut octets = ip.octets();
            octets[8..].fill(0);
            format!("{}/64", std::net::Ipv6Addr::from(octets))
        }
    }
}

fn header_family(
    headers: &HeaderMap,
    name: axum::http::header::HeaderName,
    mapper: fn(&str) -> String,
) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(mapper)
        .unwrap_or_else(|| "unknown".to_owned())
}

fn is_bot_request(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            [
                "bot",
                "crawler",
                "spider",
                "preview",
                "facebookexternalhit",
                "slackbot",
                "discordbot",
                "telegrambot",
                "whatsapp",
            ]
            .iter()
            .any(|needle| lower.contains(needle))
        })
        .unwrap_or(false)
}

fn user_agent_family(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("edg/") || lower.contains("edge/") {
        "edge".to_owned()
    } else if lower.contains("firefox/") || lower.contains("fxios/") {
        "firefox".to_owned()
    } else if lower.contains("chrome/") || lower.contains("crios/") || lower.contains("chromium/") {
        "chromium".to_owned()
    } else if lower.contains("safari/") {
        "safari".to_owned()
    } else {
        "other".to_owned()
    }
}

fn accept_language_family(value: &str) -> String {
    value
        .split(',')
        .next()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .split('-')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase()
        .chars()
        .collect::<String>()
        .if_empty("unknown")
}

trait EmptyStringExt {
    fn if_empty(self, fallback: &str) -> String;
}

impl EmptyStringExt for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_owned()
        } else {
            self
        }
    }
}

fn prune_visitor_hashes(disk: &mut AnalyticsDisk, today_index: i64) {
    for (day, stats) in &mut disk.days {
        if let Some(day_index) = parse_day_index(day)
            && today_index.saturating_sub(day_index) >= VISITOR_HASH_RETENTION_DAYS
        {
            stats.visitor_hashes.clear();
        }
    }
}

fn analytics_window(disk: &AnalyticsDisk, today_index: i64, days: i64) -> PublicAnalyticsWindow {
    let first_day = today_index.saturating_sub(days.saturating_sub(1));
    let mut window = PublicAnalyticsWindow {
        unique_visitor_days: 0,
        visits: 0,
        pageviews: 0,
    };

    for (day, stats) in &disk.days {
        if parse_day_index(day).is_some_and(|day_index| day_index >= first_day) {
            window.unique_visitor_days = window
                .unique_visitor_days
                .saturating_add(stats.unique_visitors);
            window.visits = window.visits.saturating_add(stats.visits);
            window.pageviews = window.pageviews.saturating_add(stats.pageviews);
        }
    }

    window
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn day_string(day_index: i64) -> String {
    let (year, month, day) = civil_from_days(day_index);
    format!("{year:04}-{month:02}-{day:02}")
}

fn parse_day_index(value: &str) -> Option<i64> {
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
    use axum::http::HeaderValue;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn masks_ip_prefixes() {
        assert_eq!(
            masked_ip_prefix(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 42))),
            "203.0.113.0/24"
        );
        assert_eq!(
            masked_ip_prefix(IpAddr::V6(Ipv6Addr::new(
                0x2001, 0xdb8, 0x1234, 0x5678, 0xabcd, 0, 0, 1
            ))),
            "2001:db8:1234:5678::/64"
        );
    }

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
    fn visitor_key_uses_header_families_not_raw_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 Firefox/127.0"),
        );
        headers.insert(
            axum::http::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US,en;q=0.9"),
        );
        let secret = [7u8; 32];
        let key = visitor_key(
            &secret,
            "2026-06-29",
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, 42)),
            &headers,
        );
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}
