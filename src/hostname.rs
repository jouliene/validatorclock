//! What a host name is, said once.
//!
//! A host arrives written several ways - with a port, in brackets, in capital
//! letters, with a trailing dot - and three parts of the program have to agree
//! on when two of them are the same host: the allowed-host check on every
//! request, the certificate order, and the config that validates both. It
//! lives here rather than in the server so that the config can ask without
//! depending on it.

use std::net::IpAddr;

/// The host a `https://host/...` URL names.
pub(crate) fn public_url_host(public_url: &str) -> Option<String> {
    let rest = public_url.strip_prefix("https://")?;
    let host = rest.split('/').next().unwrap_or(rest);
    normalize_host(host)
}

/// One spelling of a host: no port, no brackets, no trailing dot, lower case,
/// and an address written the way its own kind writes it.
pub(crate) fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }

    if host.starts_with('[')
        && let Some(end) = host.find(']')
    {
        let value = &host[1..end];
        return Some(
            value
                .parse::<IpAddr>()
                .map(|address| address.to_string())
                .unwrap_or_else(|_| value.to_ascii_lowercase()),
        );
    }

    if let Ok(address) = host.parse::<IpAddr>() {
        return Some(address.to_string());
    }

    let host_without_port = host
        .rsplit_once(':')
        .filter(|(name, port)| !name.contains(':') && port.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(name, _)| name)
        .unwrap_or(host);

    Some(host_without_port.trim_end_matches('.').to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_host_written_several_ways_is_one_host() {
        assert_eq!(
            normalize_host("203.0.113.10:443").as_deref(),
            Some("203.0.113.10")
        );
        assert_eq!(
            normalize_host("Example.COM.").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_host("[2001:db8::1]:443").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(
            normalize_host("2001:db8::1").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(normalize_host(" ").as_deref(), None);
    }

    #[test]
    fn a_public_url_names_its_host() {
        assert_eq!(
            public_url_host("https://validatorclock.xyz/").as_deref(),
            Some("validatorclock.xyz")
        );
        assert_eq!(
            public_url_host("https://Example.COM:443/stats").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            public_url_host("http://validatorclock.xyz/").as_deref(),
            None,
            "the certificate this names is for https, and nothing else is a public url here"
        );
    }
}
