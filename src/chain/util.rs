use super::RoundColor;
use anyhow::{Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn round_color(round_id: u32) -> RoundColor {
    if round_id.is_multiple_of(2) {
        RoundColor::Blue
    } else {
        RoundColor::Green
    }
}

pub(super) fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub(super) fn masterchain_hash_address(bytes: &[u8]) -> String {
    format!("-1:{}", hex_lower(bytes))
}

pub(super) fn endpoint_label(endpoint: &str) -> String {
    endpoint
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned()
}

pub(super) fn now_sec() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX epoch")?
        .as_secs())
}
