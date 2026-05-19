use super::AppState;
use crate::chain::{ClockSnapshot, ValidatorSetDto};
use crate::tycho_map::{TYCHO_MAP_CHAIN_ID, load_tycho_map_nodes, mapped_peer_set};
use std::collections::HashSet;
use tracing::warn;

impl AppState {
    pub(crate) fn annotate_tycho_fake_validators(&self, snapshot: &mut ClockSnapshot) {
        if snapshot.chain_id() != TYCHO_MAP_CHAIN_ID {
            return;
        }

        let nodes = match load_tycho_map_nodes(&self.config) {
            Ok(nodes) => nodes,
            Err(error) => {
                warn!(error = ?error, "failed to load Tycho map nodes for fake validator annotation");
                return;
            }
        };
        let mapped_peers = match mapped_peer_set(&nodes) {
            Ok(mapped_peers) => mapped_peers,
            Err(error) => {
                warn!(error = ?error, "failed to read Tycho map peers for fake validator annotation");
                return;
            }
        };

        annotate_set_with_fake_validators(&mut snapshot.current_set, &mapped_peers);
    }
}

fn annotate_set_with_fake_validators(set: &mut ValidatorSetDto, mapped_peers: &HashSet<String>) {
    let mut fake_peers = set
        .validators
        .iter()
        .map(|validator| validator.public_key.to_ascii_lowercase())
        .filter(|public_key| !public_key.is_empty() && !mapped_peers.contains(public_key))
        .collect::<Vec<_>>();
    fake_peers.sort();
    fake_peers.dedup();

    set.fake_validator_peers = fake_peers;
    set.fake_validator_status_known = true;
}
