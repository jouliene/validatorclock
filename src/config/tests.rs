use super::app::rpc_override_env_name;
use super::*;
use std::path::{Path, PathBuf};

fn test_chain() -> ChainConfig {
    ChainConfig {
        id: "test".to_owned(),
        name: "Test".to_owned(),
        rpc: "https://example.com".to_owned(),
        rpc_fallbacks: Vec::new(),
        color: "#38bdf8".to_owned(),
        token_symbol: "TEST".to_owned(),
        rpc_label: None,
    }
}

fn test_config() -> AppConfig {
    AppConfig::for_test(vec![test_chain()])
}

#[test]
fn derives_default_runtime_paths_from_cache_path() {
    let config = test_config();

    assert_eq!(
        config.effective_history_path(),
        PathBuf::from("/var/lib/validatorclock/validatorclock_history.json")
    );
    assert_eq!(
        config.effective_analytics_path(),
        PathBuf::from("/var/lib/validatorclock/validatorclock_analytics.json")
    );
    assert_eq!(
        config.effective_visitors_path(),
        PathBuf::from("/var/lib/validatorclock/validatorclock_visitors.json")
    );
    assert_eq!(
        config.effective_validator_type_cache_path(),
        PathBuf::from("/var/lib/validatorclock/validatorclock_validator_types.json")
    );
}

#[test]
fn explicit_history_path_overrides_default_runtime_path() {
    let mut config = test_config();
    config.history_path = Some(PathBuf::from("/state/history.json"));

    assert_eq!(
        config.effective_history_path(),
        PathBuf::from("/state/history.json")
    );
}

#[test]
fn old_config_without_node_locations_uses_disabled_defaults() {
    let config: AppConfig = serde_json::from_str(
        r##"{
            "listen": "127.0.0.1:8787",
            "refresh_seconds": 60,
            "refresh_timeout_seconds": 90,
            "cache_path": "cache.json",
            "chains": [
                {
                    "id": "test",
                    "name": "Test",
                    "rpc": "https://example.com",
                    "color": "#38bdf8",
                    "token_symbol": "TEST"
                }
            ]
        }"##,
    )
    .unwrap();

    assert!(!config.node_locations.enabled);
    assert!(
        !config
            .effective_node_location_chain("tycho-testnet")
            .enabled
    );
    assert!(config.validate().is_ok());
}

#[test]
fn node_locations_require_separate_input_and_output_paths() {
    let config: AppConfig = serde_json::from_str(
        r##"{
            "listen": "127.0.0.1:8787",
            "refresh_seconds": 60,
            "refresh_timeout_seconds": 90,
            "cache_path": "cache.json",
            "node_locations": {
                "enabled": true,
                "chains": {
                    "test": {
                        "enabled": true,
                        "input_path": ".local_maps/test_nodes.json",
                        "output_path": ".local_maps/test_nodes.json"
                    }
                }
            },
            "chains": [
                {
                    "id": "test",
                    "name": "Test",
                    "rpc": "https://example.com",
                    "color": "#38bdf8",
                    "token_symbol": "TEST"
                }
            ]
        }"##,
    )
    .unwrap();

    let error = config.validate().unwrap_err().to_string();
    assert!(error.contains("input_path must differ from output_path"));
}

/// The floor used to be applied wherever the interval was read - four places,
/// each on its own - so `/api/status` could report an interval the config never
/// asked for. It is raised once, where the config is loaded, and a zero is
/// still a mistake rather than a small number.
#[test]
fn an_interval_below_the_floor_is_raised_once_and_a_zero_is_still_refused() {
    let mut config = test_config();
    config.refresh_seconds = 3;
    config.normalize();
    assert_eq!(config.refresh_seconds, MIN_REFRESH_SECONDS);
    assert!(config.validate().is_ok());

    let mut config = test_config();
    config.refresh_seconds = 600;
    config.normalize();
    assert_eq!(
        config.refresh_seconds, 600,
        "a workable interval is left alone"
    );

    let mut config = test_config();
    config.refresh_seconds = 0;
    config.normalize();
    assert!(
        config.validate().is_err(),
        "a zero interval is a broken config, not one to fix quietly"
    );
}

#[test]
fn rejects_empty_and_missing_required_config_fields() {
    let mut config = test_config();
    config.chains.clear();
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.refresh_seconds = 0;
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.refresh_timeout_seconds = 0;
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.history_path = Some(PathBuf::new());
    assert!(config.validate().is_err());

    let mut config = test_config();
    config
        .map_nodes_paths
        .insert("ton".to_owned(), PathBuf::new());
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.chains[0].id = " ".to_owned();
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.chains[0].name = " ".to_owned();
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.chains[0].rpc = " ".to_owned();
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.chains[0].rpc_fallbacks = vec![" ".to_owned()];
    assert!(config.validate().is_err());
}

#[test]
fn rejects_unsafe_or_duplicate_chain_ids() {
    let mut config = test_config();
    config.chains[0].id = " ton".to_owned();
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.chains[0].id = "ton/mainnet".to_owned();
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.chains.push(ChainConfig {
        id: "test".to_owned(),
        name: "Duplicate Test".to_owned(),
        rpc: "https://duplicate.example.com".to_owned(),
        rpc_fallbacks: Vec::new(),
        color: "#22c55e".to_owned(),
        token_symbol: "TEST".to_owned(),
        rpc_label: None,
    });
    assert!(config.validate().is_err());
}

#[test]
fn validates_security_limits_directly() {
    let mut config = test_config();
    config.security.max_connections = 0;

    assert!(config.validate().is_err());
}

#[test]
fn tls_disabled_does_not_require_certificate_settings() {
    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: false,
        public_url: String::new(),
        cert_path: PathBuf::new(),
        key_path: PathBuf::new(),
        ..TlsConfig::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn tls_enabled_requires_https_public_url_and_key_paths() {
    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "http://example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::from("privkey.pem"),
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        cert_path: PathBuf::new(),
        key_path: PathBuf::from("privkey.pem"),
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::new(),
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn acme_public_url_must_match_identifier_or_extra_identifier() {
    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://www.example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::from("privkey.pem"),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            extra_identifiers: vec!["www.example.com".to_owned()],
            account_path: PathBuf::from("account.json"),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };
    assert!(config.validate().is_ok());
    assert_eq!(
        config.effective_allowed_hosts(),
        vec!["www.example.com".to_owned()]
    );

    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://other.example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::from("privkey.pem"),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            extra_identifiers: vec!["www.example.com".to_owned()],
            account_path: PathBuf::from("account.json"),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn acme_rejects_empty_or_invalid_enabled_settings() {
    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::from("privkey.pem"),
        acme: AcmeConfig {
            enabled: true,
            identifier: String::new(),
            account_path: PathBuf::from("account.json"),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::from("privkey.pem"),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com:443".to_owned(),
            account_path: PathBuf::from("account.json"),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::from("privkey.pem"),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            extra_identifiers: vec![String::new()],
            account_path: PathBuf::from("account.json"),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());

    let mut config = test_config();
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        cert_path: PathBuf::from("fullchain.pem"),
        key_path: PathBuf::from("privkey.pem"),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            account_path: PathBuf::new(),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn acme_default_directory_and_renewal_windows_are_stable() {
    let mut acme = AcmeConfig::default();
    assert!(
        acme.directory_url()
            .contains("acme-v02.api.letsencrypt.org")
    );
    assert_eq!(acme.renew_before_seconds(), 30 * 24 * 60 * 60);

    acme.staging = true;
    assert!(
        acme.directory_url()
            .contains("acme-staging-v02.api.letsencrypt.org")
    );

    acme.profile = Some("shortlived".to_owned());
    assert_eq!(acme.renew_before_seconds(), 2 * 24 * 60 * 60);

    acme.renew_after_seconds = Some(3600);
    assert_eq!(acme.renew_before_seconds(), 3600);
}

#[test]
fn load_config_reports_explicit_source() {
    let path = Path::new("validatorclock.json");
    let loaded = load_config(Some(path)).expect("repo default config should parse");

    assert!(matches!(loaded.source, ConfigSource::Explicit(_)));
    assert_eq!(loaded.source.label(), "explicit");
    assert_eq!(loaded.source.path(), Some(path));
    assert!(loaded.config.validate().is_ok());
    assert!(loaded.config.chain("ton").is_some());
}

#[test]
fn chain_endpoints_can_come_from_the_environment() {
    let mut config = test_config();
    let original_fallbacks = config.chains[0].rpc_fallbacks.clone();

    config.apply_endpoint_overrides_from(|variable| {
        (variable == "VALIDATORCLOCK_RPC_TEST")
            .then(|| "  https://mainnet.evercloud.dev/secret/graphql  ".to_owned())
    });

    assert_eq!(
        config.chains[0].rpc,
        "https://mainnet.evercloud.dev/secret/graphql"
    );
    assert_eq!(config.chains[0].rpc_fallbacks, original_fallbacks);
}

#[test]
fn a_blank_or_missing_override_keeps_the_configured_endpoint() {
    let mut config = test_config();
    let configured = config.chains[0].rpc.clone();

    config.apply_endpoint_overrides_from(|_| None);
    assert_eq!(config.chains[0].rpc, configured);

    config.apply_endpoint_overrides_from(|_| Some("   ".to_owned()));
    assert_eq!(config.chains[0].rpc, configured);
}

#[test]
fn override_variables_are_named_after_the_chain_id() {
    assert_eq!(
        rpc_override_env_name("everscale"),
        "VALIDATORCLOCK_RPC_EVERSCALE"
    );
    assert_eq!(
        rpc_override_env_name("tycho-testnet"),
        "VALIDATORCLOCK_RPC_TYCHO_TESTNET"
    );
}
