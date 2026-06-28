use crate::chain::ValidatorSetDto;
use std::collections::HashSet;

const NEW_SET_FAKE_STATUS_GRACE_SECONDS: u64 = 5 * 60;

enum FakeValidatorStatusUpdate {
    Deferred,
    Known(Vec<String>),
}

pub(super) fn update_set_fake_validator_status(
    set: &mut ValidatorSetDto,
    mapped_peers: &HashSet<String>,
    grace_mapped_peers: &HashSet<String>,
    map_nodes_updated_at: Option<u64>,
    observed_at: u64,
) {
    let update = fake_validator_status_update(
        set,
        mapped_peers,
        grace_mapped_peers,
        map_nodes_updated_at,
        observed_at,
    );
    update_fake_validator_status(set, update);
}

fn fake_validator_status_update(
    set: &ValidatorSetDto,
    mapped_peers: &HashSet<String>,
    grace_mapped_peers: &HashSet<String>,
    _map_nodes_updated_at: Option<u64>,
    observed_at: u64,
) -> FakeValidatorStatusUpdate {
    if should_defer_fake_validator_status(set, observed_at) {
        return FakeValidatorStatusUpdate::Deferred;
    }

    FakeValidatorStatusUpdate::Known(fake_validator_peers(set, mapped_peers, grace_mapped_peers))
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

fn fake_validator_peers(
    set: &ValidatorSetDto,
    mapped_peers: &HashSet<String>,
    grace_mapped_peers: &HashSet<String>,
) -> Vec<String> {
    let mut fake_peers = set
        .validators
        .iter()
        .map(|validator| validator.public_key.to_ascii_lowercase())
        .filter(|public_key| {
            !public_key.is_empty()
                && !mapped_peers.contains(public_key)
                && !grace_mapped_peers.contains(public_key)
        })
        .collect::<Vec<_>>();
    fake_peers.sort();
    fake_peers.dedup();
    fake_peers
}

fn should_defer_fake_validator_status(set: &ValidatorSetDto, observed_at: u64) -> bool {
    let set_started_at = u64::from(set.utime_since);
    observed_at.saturating_sub(set_started_at) < NEW_SET_FAKE_STATUS_GRACE_SECONDS
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

    #[test]
    fn fake_validator_status_is_deferred_during_new_set_grace_for_stale_map() {
        let mut set = validator_set_with_peers(&["mapped", "missing"]);
        set.utime_since = 1_000;

        update_set_fake_validator_status(
            &mut set,
            &mapped_peers(&["mapped"]),
            &HashSet::new(),
            Some(999),
            1_120,
        );

        assert!(!set.fake_validator_status_known);
        assert!(set.fake_validator_peers.is_empty());
    }

    #[test]
    fn fake_validator_status_is_deferred_during_new_set_grace_after_map_refresh() {
        let mut set = validator_set_with_peers(&["mapped", "missing"]);
        set.utime_since = 1_000;

        update_set_fake_validator_status(
            &mut set,
            &mapped_peers(&["mapped"]),
            &HashSet::new(),
            Some(1_030),
            1_120,
        );

        assert!(!set.fake_validator_status_known);
        assert!(set.fake_validator_peers.is_empty());
    }

    #[test]
    fn fake_validator_status_is_known_after_new_set_grace_expires() {
        let mut set = validator_set_with_peers(&["mapped", "missing"]);
        set.utime_since = 1_000;

        update_set_fake_validator_status(
            &mut set,
            &mapped_peers(&["mapped"]),
            &HashSet::new(),
            Some(999),
            1_301,
        );

        assert!(set.fake_validator_status_known);
        assert_eq!(set.fake_validator_peers, vec!["missing".to_owned()]);
    }

    #[test]
    fn grace_mapped_peers_are_not_marked_fake_after_new_set_grace() {
        let mut set = validator_set_with_peers(&["mapped", "grace", "missing"]);
        set.utime_since = 1_000;

        update_set_fake_validator_status(
            &mut set,
            &mapped_peers(&["mapped"]),
            &mapped_peers(&["grace"]),
            Some(999),
            1_301,
        );

        assert!(set.fake_validator_status_known);
        assert_eq!(set.fake_validator_peers, vec!["missing".to_owned()]);
    }
}
