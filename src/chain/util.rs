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

// Endpoint labels reach the public API, logs, and snapshot warnings, so path
// segments that look like API keys are masked.
pub(super) fn endpoint_label(endpoint: &str) -> String {
    let endpoint = endpoint
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');

    endpoint
        .split('/')
        .enumerate()
        .map(|(index, segment)| {
            if index > 0 && is_secret_path_segment(segment) {
                "***"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_secret_path_segment(segment: &str) -> bool {
    const MIN_SECRET_SEGMENT_LEN: usize = 16;

    segment.len() >= MIN_SECRET_SEGMENT_LEN && segment.chars().all(|ch| ch.is_ascii_alphanumeric())
}

pub(super) fn now_sec() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX epoch")?
        .as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_plain_endpoint_labels() {
        assert_eq!(
            endpoint_label("https://jrpc.everwallet.net"),
            "jrpc.everwallet.net"
        );
        assert_eq!(
            endpoint_label("https://toncenter.com/api/v2/jsonRPC"),
            "toncenter.com/api/v2/jsonRPC"
        );
        assert_eq!(
            endpoint_label("https://rpc-testnet.tychoprotocol.com/"),
            "rpc-testnet.tychoprotocol.com"
        );
    }

    #[test]
    fn masks_api_key_path_segments() {
        assert_eq!(
            endpoint_label(
                "https://mainnet.evercloud.dev/89a3b8f46a484f2ea3bdd364ddaee3a3/graphql"
            ),
            "mainnet.evercloud.dev/***/graphql"
        );
    }
}
