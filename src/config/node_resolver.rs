use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Finding out where validators run, which the project used to ask a second
/// program to do. Off unless a chain is configured for it, so a deployment
/// that still runs the external resolver is unaffected by this being here.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct NodeResolverConfig {
    pub(crate) enabled: bool,
    /// How often to ask the DHT about the whole validator set again.
    pub(crate) refresh_seconds: u64,
    /// Long enough after startup that the first chain refresh has a validator
    /// set to hand over. Without one there is nothing to resolve.
    pub(crate) startup_delay_seconds: u64,
    /// The local address the DHT answers on. It is a UDP socket, and only one
    /// process can hold it - while the external resolver still runs, this has
    /// to be a different port than the one it uses.
    pub(crate) local_adnl_addr: String,
    /// How long one validator's lookup may take before it is called a miss.
    pub(crate) lookup_timeout_seconds: u64,
    /// How many validators are looked up at once.
    pub(crate) workers: usize,
    pub(crate) chains: HashMap<String, NodeResolverChainConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct NodeResolverChainConfig {
    pub(crate) enabled: bool,
    /// The chain's global config, for the DHT bootstrap peers.
    pub(crate) global_config_path: Option<PathBuf>,
    /// Where the resolved set is written. This is the file the node location
    /// map reads, so the result survives a restart and the page has addresses
    /// to draw before the DHT has said a word.
    pub(crate) output_path: Option<PathBuf>,
}

impl Default for NodeResolverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            refresh_seconds: 300,
            startup_delay_seconds: 30,
            local_adnl_addr: "0.0.0.0:4191".to_owned(),
            lookup_timeout_seconds: 30,
            workers: 16,
            chains: HashMap::new(),
        }
    }
}

impl NodeResolverConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.refresh_seconds == 0 {
            bail!("node_resolver.refresh_seconds must be greater than zero");
        }
        if self.lookup_timeout_seconds == 0 {
            bail!("node_resolver.lookup_timeout_seconds must be greater than zero");
        }
        if self.workers == 0 {
            bail!("node_resolver.workers must be greater than zero");
        }
        if self.local_adnl_addr.trim().is_empty() {
            bail!("node_resolver.local_adnl_addr cannot be empty");
        }
        for (chain_id, chain) in &self.chains {
            if !chain.enabled {
                continue;
            }
            if chain.global_config_path.is_none() {
                bail!("node_resolver chain `{chain_id}` needs a global_config_path");
            }
            if chain.output_path.is_none() {
                bail!("node_resolver chain `{chain_id}` needs an output_path");
            }
        }
        Ok(())
    }

    /// The chains this resolver is actually meant to run for.
    pub(crate) fn active_chains(&self) -> Vec<(&str, &NodeResolverChainConfig)> {
        if !self.enabled {
            return Vec::new();
        }
        let mut chains = self
            .chains
            .iter()
            .filter(|(_, chain)| chain.enabled)
            .map(|(id, chain)| (id.as_str(), chain))
            .collect::<Vec<_>>();
        chains.sort_by_key(|(id, _)| *id);
        chains
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_chain() -> NodeResolverChainConfig {
        NodeResolverChainConfig {
            enabled: true,
            global_config_path: Some(PathBuf::from("/tmp/global.json")),
            output_path: Some(PathBuf::from("/tmp/out.json")),
        }
    }

    #[test]
    fn a_resolver_that_is_off_is_not_validated_or_run() {
        let config = NodeResolverConfig {
            workers: 0,
            chains: HashMap::from([("everscale".to_owned(), enabled_chain())]),
            ..NodeResolverConfig::default()
        };

        assert!(
            config.validate().is_ok(),
            "an unused section is not a fault"
        );
        assert!(config.active_chains().is_empty());
    }

    #[test]
    fn an_enabled_chain_must_say_where_to_bootstrap_and_where_to_write() {
        let missing_config_path = NodeResolverConfig {
            enabled: true,
            chains: HashMap::from([(
                "everscale".to_owned(),
                NodeResolverChainConfig {
                    global_config_path: None,
                    ..enabled_chain()
                },
            )]),
            ..NodeResolverConfig::default()
        };
        assert!(missing_config_path.validate().is_err());

        let missing_output = NodeResolverConfig {
            enabled: true,
            chains: HashMap::from([(
                "everscale".to_owned(),
                NodeResolverChainConfig {
                    output_path: None,
                    ..enabled_chain()
                },
            )]),
            ..NodeResolverConfig::default()
        };
        assert!(missing_output.validate().is_err());
    }

    #[test]
    fn only_chains_switched_on_are_run() {
        let config = NodeResolverConfig {
            enabled: true,
            chains: HashMap::from([
                ("everscale".to_owned(), enabled_chain()),
                (
                    "ton".to_owned(),
                    NodeResolverChainConfig {
                        enabled: false,
                        ..enabled_chain()
                    },
                ),
            ]),
            ..NodeResolverConfig::default()
        };

        assert!(config.validate().is_ok());
        let active = config.active_chains();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0, "everscale");
    }
}
