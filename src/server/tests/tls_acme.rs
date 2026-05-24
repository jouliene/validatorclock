use super::*;
use crate::config::{AcmeConfig, TlsConfig};
use crate::tls;

#[test]
fn rejects_acme_identifier_with_port() {
    assert!(tls::acme_identifier("203.0.113.10").is_ok());
    assert!(tls::acme_identifier("example.com").is_ok());
    assert!(tls::acme_identifier("example.com:443").is_err());
    assert!(tls::acme_identifier("https://example.com").is_err());
    assert!(tls::acme_identifier("[2001:db8::1]").is_err());
}

#[test]
fn tls_public_url_can_match_extra_acme_identifier() {
    let mut config = test_config(Vec::new());
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://www.example.com".to_owned(),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            extra_identifiers: vec!["www.example.com".to_owned()],
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn tls_public_url_must_match_one_acme_identifier() {
    let mut config = test_config(Vec::new());
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://other.example.com".to_owned(),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            extra_identifiers: vec!["www.example.com".to_owned()],
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };

    assert!(config.validate().is_err());
}

#[test]
fn acme_profile_is_optional_for_domain_certificates() {
    let mut config = test_config(Vec::new());
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };

    assert!(config.validate().is_ok());
    assert_eq!(config.tls.acme.profile_value(), None);
    assert_eq!(config.tls.acme.renew_before_seconds(), 30 * 24 * 60 * 60);
}

#[test]
fn shortlived_profile_uses_short_default_renewal_window() {
    let acme = AcmeConfig {
        profile: Some("shortlived".to_owned()),
        ..AcmeConfig::default()
    };

    assert_eq!(acme.renew_before_seconds(), 2 * 24 * 60 * 60);
}

#[test]
fn refresh_timeout_must_be_positive() {
    let mut config = test_config(Vec::new());
    config.refresh_timeout_seconds = 0;

    assert!(config.validate().is_err());
}
