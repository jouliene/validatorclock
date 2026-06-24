use super::*;
use std::collections::HashMap;
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
    AppConfig {
        listen: "127.0.0.1:8787".to_owned(),
        refresh_seconds: 60,
        refresh_timeout_seconds: 90,
        cache_path: PathBuf::from("/var/lib/validatorclock/cache.json"),
        history_path: None,
        tycho_map_nodes_path: None,
        map_nodes_paths: HashMap::new(),
        node_locations: Default::default(),
        security: SecurityConfig::default(),
        tls: TlsConfig::default(),
        chains: vec![test_chain()],
    }
}

#[test]
fn derives_default_runtime_paths_from_cache_path() {
    let config = test_config();

    assert_eq!(
        config.effective_history_path(),
        PathBuf::from("/var/lib/validatorclock/validatorclock_history.json")
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
    config.tycho_map_nodes_path = Some(PathBuf::new());
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
