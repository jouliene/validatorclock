use super::AppState;
use crate::chain::{ClockSnapshot, ValidatorMapNodeDto, ValidatorSetDto};
use crate::validator_map::{load_map_nodes_with_metadata, map_nodes_by_peer};
use std::collections::{HashMap, HashSet};
use tracing::warn;

mod fake_status;

use fake_status::update_set_fake_validator_status;

struct ValidatorMapAnnotations {
    nodes_by_peer: HashMap<String, ValidatorMapNodeDto>,
    mapped_peers: HashSet<String>,
    updated_at: Option<u64>,
}

impl AppState {
    pub(crate) async fn annotate_map_fake_validators(
        &self,
        snapshot: &mut ClockSnapshot,
        observed_at: u64,
    ) {
        let chain_id = snapshot.chain_id();
        let Some(annotations) = self.load_validator_map_annotations(chain_id) else {
            return;
        };

        let grace_mapped_peers = self
            .round_history
            .read()
            .await
            .recent_mapped_validator_peers(chain_id, &snapshot.current_set, observed_at);

        annotate_set_with_validator_map(
            &mut snapshot.current_set,
            &annotations,
            &grace_mapped_peers,
            observed_at,
        );
    }

    fn load_validator_map_annotations(&self, chain_id: &str) -> Option<ValidatorMapAnnotations> {
        let payload = match load_map_nodes_with_metadata(&self.config, chain_id) {
            Ok(Some(payload)) => payload,
            Ok(None) => return None,
            Err(error) => {
                warn!(
                    chain_id,
                    error = ?error,
                    "failed to load map nodes for fake validator annotation"
                );
                return None;
            }
        };

        let nodes_by_peer = match map_nodes_by_peer(&payload.nodes) {
            Ok(map_nodes) => map_nodes,
            Err(error) => {
                warn!(
                    chain_id,
                    error = ?error,
                    "failed to read map nodes for fake validator annotation"
                );
                return None;
            }
        };

        Some(ValidatorMapAnnotations {
            mapped_peers: nodes_by_peer.keys().cloned().collect(),
            nodes_by_peer,
            updated_at: payload.updated_at,
        })
    }
}

fn annotate_set_with_validator_map(
    set: &mut ValidatorSetDto,
    annotations: &ValidatorMapAnnotations,
    grace_mapped_peers: &HashSet<String>,
    observed_at: u64,
) {
    annotate_set_with_map_nodes(set, &annotations.nodes_by_peer);
    update_set_fake_validator_status(
        set,
        &annotations.mapped_peers,
        grace_mapped_peers,
        annotations.updated_at,
        observed_at,
    );
}

fn annotate_set_with_map_nodes(
    set: &mut ValidatorSetDto,
    map_nodes: &HashMap<String, ValidatorMapNodeDto>,
) {
    for validator in &mut set.validators {
        let public_key = validator.public_key.to_ascii_lowercase();
        if let Some(map_node) = map_nodes.get(&public_key) {
            validator.map_node = Some(map_node.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::test_clock_snapshot;

    fn validator_set_with_peers(peers: &[&str]) -> ValidatorSetDto {
        let mut snapshot = test_clock_snapshot("ton");
        let template = snapshot.current_set.validators[0].clone();
        snapshot.current_set.validators = peers
            .iter()
            .map(|peer| {
                let mut validator = template.clone();
                validator.public_key = (*peer).to_owned();
                validator
            })
            .collect();
        snapshot.current_set
    }

    fn map_node(ip: &str) -> ValidatorMapNodeDto {
        ValidatorMapNodeDto {
            ip: Some(ip.to_owned()),
            isp: Some("Example ISP".to_owned()),
            city: Some("Example City".to_owned()),
            country: Some("Example Country".to_owned()),
        }
    }

    #[test]
    fn map_nodes_are_applied_by_validator_public_key() {
        let mut set = validator_set_with_peers(&["mapped", "missing"]);
        let map_node = map_node("192.0.2.10");
        let map_nodes = HashMap::from([("mapped".to_owned(), map_node.clone())]);

        annotate_set_with_map_nodes(&mut set, &map_nodes);

        assert_eq!(set.validators[0].map_node, Some(map_node));
        assert_eq!(set.validators[1].map_node, None);
    }
}
