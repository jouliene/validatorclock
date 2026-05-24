use super::AppState;
use crate::chain::{ClockSnapshot, ValidatorMapNodeDto, ValidatorSetDto};
use crate::validator_map::{load_map_nodes_with_metadata, map_nodes_by_peer};
use std::collections::{HashMap, HashSet};
use tracing::warn;

const NEW_SET_FAKE_STATUS_GRACE_SECONDS: u64 = 5 * 60;

struct ValidatorMapAnnotations {
    nodes_by_peer: HashMap<String, ValidatorMapNodeDto>,
    mapped_peers: HashSet<String>,
    updated_at: Option<u64>,
}

enum FakeValidatorStatusUpdate {
    Deferred,
    Known(Vec<String>),
}

impl AppState {
    pub(crate) fn annotate_map_fake_validators(
        &self,
        snapshot: &mut ClockSnapshot,
        observed_at: u64,
    ) {
        let chain_id = snapshot.chain_id();
        let Some(annotations) = self.load_validator_map_annotations(chain_id) else {
            return;
        };

        annotate_set_with_validator_map(&mut snapshot.current_set, &annotations, observed_at);
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
    observed_at: u64,
) {
    annotate_set_with_map_nodes(set, &annotations.nodes_by_peer);
    let fake_status_update = fake_validator_status_update(
        set,
        &annotations.mapped_peers,
        annotations.updated_at,
        observed_at,
    );
    update_fake_validator_status(set, fake_status_update);
}

fn fake_validator_status_update(
    set: &ValidatorSetDto,
    mapped_peers: &HashSet<String>,
    map_nodes_updated_at: Option<u64>,
    observed_at: u64,
) -> FakeValidatorStatusUpdate {
    if should_defer_fake_validator_status(set, map_nodes_updated_at, observed_at) {
        return FakeValidatorStatusUpdate::Deferred;
    }

    FakeValidatorStatusUpdate::Known(fake_validator_peers(set, mapped_peers))
}

fn update_fake_validator_status(set: &mut ValidatorSetDto, update: FakeValidatorStatusUpdate) {
    match update {
        FakeValidatorStatusUpdate::Deferred => {
            set.fake_validator_peers.clear();
            set.fake_validator_status_known = false;
        }
        FakeValidatorStatusUpdate::Known(fake_peers) => {
            set.fake_validator_peers = fake_peers;
            set.fake_validator_status_known = true;
        }
    }
}

fn fake_validator_peers(set: &ValidatorSetDto, mapped_peers: &HashSet<String>) -> Vec<String> {
    let mut fake_peers = set
        .validators
        .iter()
        .map(|validator| validator.public_key.to_ascii_lowercase())
        .filter(|public_key| !public_key.is_empty() && !mapped_peers.contains(public_key))
        .collect::<Vec<_>>();
    fake_peers.sort();
    fake_peers.dedup();
    fake_peers
}

fn should_defer_fake_validator_status(
    set: &ValidatorSetDto,
    map_nodes_updated_at: Option<u64>,
    observed_at: u64,
) -> bool {
    let set_started_at = u64::from(set.utime_since);
    if observed_at.saturating_sub(set_started_at) >= NEW_SET_FAKE_STATUS_GRACE_SECONDS {
        return false;
    }

    match map_nodes_updated_at {
        Some(updated_at) => updated_at < set_started_at,
        None => true,
    }
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

    fn mapped_peers(peers: &[&str]) -> HashSet<String> {
        peers.iter().map(|peer| (*peer).to_owned()).collect()
    }

    fn map_node(ip: &str) -> ValidatorMapNodeDto {
        ValidatorMapNodeDto {
            ip: Some(ip.to_owned()),
            isp: Some("Example ISP".to_owned()),
            city: Some("Example City".to_owned()),
            country: Some("Example Country".to_owned()),
        }
    }

    fn apply_fake_status(
        set: &mut ValidatorSetDto,
        mapped_peers: &HashSet<String>,
        map_nodes_updated_at: Option<u64>,
        observed_at: u64,
    ) {
        let update =
            fake_validator_status_update(set, mapped_peers, map_nodes_updated_at, observed_at);
        update_fake_validator_status(set, update);
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

    #[test]
    fn fake_validator_status_is_deferred_during_new_set_grace_for_stale_map() {
        let mut set = validator_set_with_peers(&["mapped", "missing"]);
        set.utime_since = 1_000;

        apply_fake_status(&mut set, &mapped_peers(&["mapped"]), Some(999), 1_120);

        assert!(!set.fake_validator_status_known);
        assert!(set.fake_validator_peers.is_empty());
    }

    #[test]
    fn fake_validator_status_is_known_during_grace_after_map_refresh() {
        let mut set = validator_set_with_peers(&["mapped", "missing"]);
        set.utime_since = 1_000;

        apply_fake_status(&mut set, &mapped_peers(&["mapped"]), Some(1_030), 1_120);

        assert!(set.fake_validator_status_known);
        assert_eq!(set.fake_validator_peers, vec!["missing".to_owned()]);
    }

    #[test]
    fn fake_validator_status_is_known_after_new_set_grace_expires() {
        let mut set = validator_set_with_peers(&["mapped", "missing"]);
        set.utime_since = 1_000;

        apply_fake_status(&mut set, &mapped_peers(&["mapped"]), Some(999), 1_301);

        assert!(set.fake_validator_status_known);
        assert_eq!(set.fake_validator_peers, vec!["missing".to_owned()]);
    }
}
