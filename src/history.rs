use crate::chain::{ClockSnapshot, RoundColor, ValidatorSetDto};
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result, bail};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const HISTORY_DEPTH: usize = 5;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ParticipationStatus {
    Participated,
    Missed,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidatorParticipationDto {
    round: u32,
    status: ParticipationStatus,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RecentAbsentValidatorDto {
    public_key: String,
    wallet: Option<String>,
    last_seen_round: u32,
    history: Vec<ValidatorParticipationDto>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RoundHistoryStore {
    #[serde(default)]
    chains: HashMap<String, ChainRoundHistory>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
struct ChainRoundHistory {
    #[serde(default)]
    rounds: BTreeMap<u32, StoredRound>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct StoredRound {
    round_id: u32,
    round_color: RoundColor,
    utime_since: u32,
    utime_until: u32,
    observed_at: u64,
    #[serde(
        default = "default_complete_history_round",
        skip_serializing_if = "is_complete_history_round"
    )]
    complete: bool,
    #[serde(default, deserialize_with = "deserialize_stored_validators")]
    validators: BTreeMap<String, StoredValidator>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
struct StoredValidator {
    wallet: Option<String>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
struct RoundHistoryDisk {
    version: u32,
    #[serde(default)]
    chains: HashMap<String, ChainRoundHistory>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub(crate) struct RoundHistoryRetention {
    chain_rounds: HashMap<String, BTreeSet<u32>>,
}

impl RoundHistoryRetention {
    fn from_snapshot(chain_id: &str, snapshot: &ClockSnapshot) -> Self {
        let mut retention = Self::default();
        retention.add_set(chain_id, &snapshot.current_set);
        if let Some(previous_set) = &snapshot.previous_set {
            retention.add_set(chain_id, previous_set);
        }
        if let Some(next_set) = &snapshot.next_set {
            retention.add_set(chain_id, next_set);
        }

        let election_round_id = snapshot.current_set.round_id.saturating_add(1);
        retention.add_round_window(chain_id, election_round_id);
        retention
    }

    fn add_set(&mut self, chain_id: &str, set: &ValidatorSetDto) {
        self.add_round_window(chain_id, set.round_id);
    }

    fn add_round_window(&mut self, chain_id: &str, round_id: u32) {
        self.chain_rounds
            .entry(chain_id.to_owned())
            .or_default()
            .extend(same_color_rounds(round_id));
    }
}

impl RoundHistoryStore {
    pub(crate) fn retention_for_snapshot(
        chain_id: &str,
        snapshot: &ClockSnapshot,
    ) -> RoundHistoryRetention {
        RoundHistoryRetention::from_snapshot(chain_id, snapshot)
    }

    pub(crate) fn merge_from(&mut self, other: RoundHistoryStore) -> bool {
        let mut changed = false;
        for (chain_id, other_chain) in other.chains {
            changed |= self
                .chains
                .entry(chain_id)
                .or_default()
                .merge_from(other_chain);
        }
        changed
    }

    pub(crate) fn record_snapshot(
        &mut self,
        chain_id: &str,
        snapshot: &ClockSnapshot,
        observed_at: u64,
    ) -> bool {
        let retention = RoundHistoryRetention::from_snapshot(chain_id, snapshot);
        let chain = self.chains.entry(chain_id.to_owned()).or_default();
        let mut changed = chain.record_set(&snapshot.current_set, observed_at);
        if let Some(previous_set) = &snapshot.previous_set {
            changed |= chain.record_set(previous_set, observed_at);
        }
        if let Some(next_set) = &snapshot.next_set {
            changed |= chain.record_set(next_set, observed_at);
        }
        changed |= self.prune_to_retention(&retention);
        changed
    }

    pub(crate) fn annotate_snapshot(&self, chain_id: &str, snapshot: &mut ClockSnapshot) {
        self.annotate_set(chain_id, &mut snapshot.current_set);
        if let Some(previous_set) = &mut snapshot.previous_set {
            self.annotate_set(chain_id, previous_set);
        }
        if let Some(next_set) = &mut snapshot.next_set {
            self.annotate_set(chain_id, next_set);
        }
        self.annotate_election_candidates(chain_id, snapshot);
    }

    fn annotate_set(&self, chain_id: &str, set: &mut ValidatorSetDto) {
        let current_validators = ValidatorIdentitySet::from_validators(&set.validators);

        for validator in &mut set.validators {
            validator.history = self.same_color_participation(
                chain_id,
                set.round_id,
                set.round_color,
                &validator.public_key,
                validator.wallet.as_deref(),
            );
        }

        set.recent_absent_validators = self.recent_absent_validators(
            chain_id,
            set.round_id,
            set.round_color,
            &current_validators,
        );
    }

    fn annotate_election_candidates(&self, chain_id: &str, snapshot: &mut ClockSnapshot) {
        if snapshot.election.candidates.is_empty() {
            return;
        }

        let election_round_id = snapshot.current_set.round_id.saturating_add(1);
        let election_round_color = opposite_round_color(snapshot.current_set.round_color);
        for candidate in &mut snapshot.election.candidates {
            candidate.history = self.same_color_participation(
                chain_id,
                election_round_id,
                election_round_color,
                &candidate.public_key,
                Some(candidate.wallet.as_str()),
            );
        }
    }

    fn same_color_participation(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
        public_key: &str,
        wallet: Option<&str>,
    ) -> Vec<ValidatorParticipationDto> {
        let chain = self.chains.get(chain_id);
        same_color_rounds(round_id)
            .into_iter()
            .map(|round| {
                let status = chain
                    .and_then(|chain| chain.rounds.get(&round))
                    .filter(|stored| stored.round_color == round_color)
                    .map(|stored| {
                        if stored.contains_identity(public_key, wallet) {
                            ParticipationStatus::Participated
                        } else if stored.complete {
                            ParticipationStatus::Missed
                        } else {
                            ParticipationStatus::Unknown
                        }
                    })
                    .unwrap_or(ParticipationStatus::Unknown);
                ValidatorParticipationDto { round, status }
            })
            .collect()
    }

    fn recent_absent_validators(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
        current_validators: &ValidatorIdentitySet,
    ) -> Vec<RecentAbsentValidatorDto> {
        let Some(chain) = self.chains.get(chain_id) else {
            return Vec::new();
        };

        let mut recent = BTreeMap::<String, RecentAbsentValidatorDto>::new();
        for round in same_color_rounds(round_id) {
            let Some(stored) = chain
                .rounds
                .get(&round)
                .filter(|stored| stored.round_color == round_color && stored.complete)
            else {
                continue;
            };

            for (public_key, validator) in &stored.validators {
                if current_validators.contains(public_key, validator.wallet.as_deref()) {
                    continue;
                }

                let recent_key = validator
                    .wallet
                    .clone()
                    .unwrap_or_else(|| public_key.clone());
                recent
                    .entry(recent_key)
                    .and_modify(|summary| {
                        summary.last_seen_round = round;
                        summary.public_key = public_key.clone();
                        if summary.wallet.is_none() {
                            summary.wallet = validator.wallet.clone();
                        }
                    })
                    .or_insert_with(|| RecentAbsentValidatorDto {
                        public_key: public_key.clone(),
                        wallet: validator.wallet.clone(),
                        last_seen_round: round,
                        history: Vec::new(),
                    });
            }
        }

        let mut recent: Vec<_> = recent
            .into_values()
            .map(|mut validator| {
                validator.history = self.same_color_participation(
                    chain_id,
                    round_id,
                    round_color,
                    &validator.public_key,
                    validator.wallet.as_deref(),
                );
                validator
            })
            .collect();
        recent.sort_by(|a, b| {
            b.last_seen_round
                .cmp(&a.last_seen_round)
                .then_with(|| a.public_key.cmp(&b.public_key))
        });
        recent
    }

    pub(crate) fn prune_to_retention(&mut self, retention: &RoundHistoryRetention) -> bool {
        let mut changed = false;
        for (chain_id, keep_rounds) in &retention.chain_rounds {
            if let Some(chain) = self.chains.get_mut(chain_id) {
                changed |= chain.prune_to_rounds(keep_rounds);
            }
        }
        changed |= self.remove_empty_chains();
        changed
    }

    fn remove_incomplete_rounds(&mut self) -> bool {
        let mut changed = false;
        for chain in self.chains.values_mut() {
            changed |= chain.remove_incomplete_rounds();
        }
        changed |= self.remove_empty_chains();
        changed
    }

    fn remove_empty_chains(&mut self) -> bool {
        let before = self.chains.len();
        self.chains.retain(|_, chain| !chain.rounds.is_empty());
        self.chains.len() != before
    }
}

struct ValidatorIdentitySet {
    public_keys: BTreeSet<String>,
    wallets: BTreeSet<String>,
}

impl ValidatorIdentitySet {
    fn from_validators(validators: &[crate::chain::ValidatorDto]) -> Self {
        Self {
            public_keys: validators
                .iter()
                .map(|validator| validator.public_key.clone())
                .collect(),
            wallets: validators
                .iter()
                .filter_map(|validator| validator.wallet.clone())
                .collect(),
        }
    }

    fn contains(&self, public_key: &str, wallet: Option<&str>) -> bool {
        self.public_keys.contains(public_key)
            || wallet.is_some_and(|wallet| self.wallets.contains(wallet))
    }
}

impl ChainRoundHistory {
    fn merge_from(&mut self, other: ChainRoundHistory) -> bool {
        let mut changed = false;
        for (round_id, other_round) in other.rounds {
            if !other_round.complete {
                continue;
            }
            match self.rounds.get_mut(&round_id) {
                Some(round) => changed |= round.merge_from(other_round),
                None => {
                    self.rounds.insert(round_id, other_round);
                    changed = true;
                }
            }
        }
        changed
    }

    fn record_set(&mut self, set: &ValidatorSetDto, observed_at: u64) -> bool {
        if set.validators.is_empty() {
            return false;
        }

        let incoming = StoredRound {
            round_id: set.round_id,
            round_color: set.round_color,
            utime_since: set.utime_since,
            utime_until: set.utime_until,
            observed_at,
            complete: true,
            validators: set
                .validators
                .iter()
                .map(|validator| {
                    (
                        validator.public_key.clone(),
                        StoredValidator {
                            wallet: validator.wallet.clone(),
                        },
                    )
                })
                .collect(),
        };

        match self.rounds.get_mut(&set.round_id) {
            Some(existing) => existing.merge_from(incoming),
            None => {
                self.rounds.insert(set.round_id, incoming);
                true
            }
        }
    }

    fn prune_to_rounds(&mut self, keep_rounds: &BTreeSet<u32>) -> bool {
        let before = self.rounds.len();
        self.rounds
            .retain(|round_id, round| keep_rounds.contains(round_id) && round.complete);
        self.rounds.len() != before
    }

    fn remove_incomplete_rounds(&mut self) -> bool {
        let before = self.rounds.len();
        self.rounds.retain(|_, round| round.complete);
        self.rounds.len() != before
    }
}

impl StoredRound {
    fn contains_identity(&self, public_key: &str, wallet: Option<&str>) -> bool {
        self.validators.contains_key(public_key)
            || wallet.is_some_and(|wallet| {
                self.validators
                    .values()
                    .any(|validator| validator.wallet.as_deref() == Some(wallet))
            })
    }

    fn merge_from(&mut self, other: StoredRound) -> bool {
        if !other.complete {
            return false;
        }
        if !self.complete {
            return self.replace_with_preserved_wallets(other);
        }
        self.merge_complete_from(other)
    }

    fn merge_complete_from(&mut self, other: StoredRound) -> bool {
        let other_is_preferred = other.observed_at > self.observed_at
            || (other.observed_at == self.observed_at && other.richness() > self.richness());
        if other_is_preferred {
            self.replace_with_preserved_wallets(other)
        } else {
            self.merge_missing_wallets(other)
        }
    }

    fn replace_with_preserved_wallets(&mut self, mut replacement: StoredRound) -> bool {
        for (public_key, validator) in &mut replacement.validators {
            if validator.wallet.is_none()
                && let Some(wallet) = self
                    .validators
                    .get(public_key)
                    .and_then(|existing| existing.wallet.clone())
            {
                validator.wallet = Some(wallet);
            }
        }

        if self.same_meaningful_content(&replacement) {
            return false;
        }

        *self = replacement;
        true
    }

    fn merge_missing_wallets(&mut self, other: StoredRound) -> bool {
        let mut changed = false;
        let observed_at = other.observed_at;
        for (public_key, other_validator) in other.validators {
            if let Some(validator) = self.validators.get_mut(&public_key)
                && validator.wallet.is_none()
                && other_validator.wallet.is_some()
            {
                validator.wallet = other_validator.wallet;
                changed = true;
            }
        }
        if changed {
            self.observed_at = self.observed_at.max(observed_at);
        }
        changed
    }

    fn same_meaningful_content(&self, other: &StoredRound) -> bool {
        self.round_id == other.round_id
            && self.round_color == other.round_color
            && self.utime_since == other.utime_since
            && self.utime_until == other.utime_until
            && self.complete == other.complete
            && self.validators == other.validators
    }

    fn richness(&self) -> (usize, usize) {
        (
            self.validators.len(),
            self.validators
                .values()
                .filter(|validator| validator.wallet.is_some())
                .count(),
        )
    }
}

fn default_complete_history_round() -> bool {
    true
}

fn is_complete_history_round(complete: &bool) -> bool {
    *complete
}

fn same_color_rounds(round_id: u32) -> Vec<u32> {
    (0..HISTORY_DEPTH)
        .rev()
        .filter_map(|index| round_id.checked_sub((index * 2) as u32))
        .collect()
}

fn opposite_round_color(round_color: RoundColor) -> RoundColor {
    match round_color {
        RoundColor::Blue => RoundColor::Green,
        RoundColor::Green => RoundColor::Blue,
    }
}

fn deserialize_stored_validators<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, StoredValidator>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StoredValidatorsCompat {
        Map(BTreeMap<String, StoredValidator>),
        List(BTreeSet<String>),
    }

    match StoredValidatorsCompat::deserialize(deserializer)? {
        StoredValidatorsCompat::Map(validators) => Ok(validators),
        StoredValidatorsCompat::List(validators) => Ok(validators
            .into_iter()
            .map(|public_key| (public_key, StoredValidator::default()))
            .collect()),
    }
}

pub(crate) fn load_round_history(path: &Path) -> Result<RoundHistoryStore> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(RoundHistoryStore::default());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let disk: RoundHistoryDisk = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let mut history = RoundHistoryStore {
        chains: disk.chains,
    };
    history.remove_incomplete_rounds();
    Ok(history)
}

pub(crate) fn save_round_history(path: &Path, history: &RoundHistoryStore) -> Result<()> {
    let disk = RoundHistoryDisk {
        version: 1,
        chains: history.chains.clone(),
    };
    let content = serde_json::to_string_pretty(&disk)?;
    write_file_atomic(path, content.as_bytes(), 0o644)
}

pub(crate) fn save_round_history_merged(
    path: &Path,
    history: &mut RoundHistoryStore,
    retention: &RoundHistoryRetention,
) -> Result<()> {
    let _lock = RoundHistoryFileLock::acquire(path)?;
    let disk_history = load_round_history(path)?;
    history.merge_from(disk_history);
    history.prune_to_retention(retention);
    save_round_history(path, history)
}

struct RoundHistoryFileLock {
    path: PathBuf,
}

impl RoundHistoryFileLock {
    fn acquire(history_path: &Path) -> Result<Self> {
        if let Some(parent) = history_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let lock_path = round_history_lock_path(history_path);
        let started_at = Instant::now();
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(file) => {
                    let _ = file.set_len(0);
                    return Ok(Self { path: lock_path });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if lock_file_is_stale(&lock_path, Duration::from_secs(300)) {
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if started_at.elapsed() > Duration::from_secs(120) {
                        bail!("timed out waiting for {}", lock_path.display());
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to lock {}", history_path.display()));
                }
            }
        }
    }
}

impl Drop for RoundHistoryFileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn round_history_lock_path(history_path: &Path) -> PathBuf {
    let mut lock_path = history_path.to_path_buf();
    let file_name = history_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.lock"))
        .unwrap_or_else(|| ".validators_clock_history.lock".to_owned());
    lock_path.set_file_name(file_name);
    lock_path
}

fn lock_file_is_stale(path: &Path, stale_after: Duration) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > stale_after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{RoundColor, ValidatorDto};

    fn validator(public_key: &str) -> ValidatorDto {
        validator_with_wallet(public_key, None)
    }

    fn validator_with_wallet(public_key: &str, wallet: Option<&str>) -> ValidatorDto {
        ValidatorDto {
            public_key: public_key.to_owned(),
            adnl_addr: None,
            wallet: wallet.map(str::to_owned),
            source: None,
            contract_type: None,
            contract_type_hash: None,
            stake: None,
            reward: None,
            weight: "1".to_owned(),
            weight_percent: 100.0,
            history: Vec::new(),
        }
    }

    fn set(round_id: u32, round_color: RoundColor, validators: Vec<&str>) -> ValidatorSetDto {
        ValidatorSetDto {
            utime_since: round_id * 10,
            utime_until: round_id * 10 + 10,
            round_id,
            round_color,
            total: validators.len(),
            main: validators.len() as u16,
            total_weight: validators.len().to_string(),
            total_stake: None,
            total_reward: None,
            validators: validators.into_iter().map(validator).collect(),
            recent_absent_validators: Vec::new(),
        }
    }

    fn stored_round(set: &ValidatorSetDto, observed_at: u64, complete: bool) -> StoredRound {
        StoredRound {
            round_id: set.round_id,
            round_color: set.round_color,
            utime_since: set.utime_since,
            utime_until: set.utime_until,
            observed_at,
            complete,
            validators: set
                .validators
                .iter()
                .map(|validator| {
                    (
                        validator.public_key.clone(),
                        StoredValidator {
                            wallet: validator.wallet.clone(),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn same_color_participation_marks_known_misses_and_unknowns() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
        chain.record_set(&set(8, RoundColor::Blue, vec!["alice"]), 100);
        chain.record_set(&set(6, RoundColor::Blue, vec!["bob"]), 100);

        let history = store.same_color_participation("test", 10, RoundColor::Blue, "alice", None);
        assert!(matches!(history[0].status, ParticipationStatus::Unknown));
        assert!(matches!(history[1].status, ParticipationStatus::Unknown));
        assert!(matches!(history[2].status, ParticipationStatus::Missed));
        assert!(matches!(
            history[3].status,
            ParticipationStatus::Participated
        ));
        assert!(matches!(
            history[4].status,
            ParticipationStatus::Participated
        ));
    }

    #[test]
    fn recent_absent_validators_lists_recent_participants_missing_now() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
        chain.record_set(&set(8, RoundColor::Blue, vec!["alice", "bob"]), 100);
        chain.record_set(&set(6, RoundColor::Blue, vec!["carol"]), 100);

        let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);
        assert_eq!(absent.len(), 2);
        assert_eq!(absent[0].public_key, "bob");
        assert_eq!(absent[0].last_seen_round, 8);
        assert_eq!(absent[1].public_key, "carol");
        assert_eq!(absent[1].last_seen_round, 6);
        assert!(matches!(
            absent[0].history[4].status,
            ParticipationStatus::Missed
        ));
    }

    #[test]
    fn legacy_incomplete_round_history_never_marks_missing_validators_as_missed() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
        let legacy_round = set(8, RoundColor::Blue, vec!["bob"]);
        chain
            .rounds
            .insert(8, stored_round(&legacy_round, 100, false));

        let history = store.same_color_participation("test", 10, RoundColor::Blue, "alice", None);

        assert!(matches!(history[3].status, ParticipationStatus::Unknown));
        assert!(matches!(
            history[4].status,
            ParticipationStatus::Participated
        ));
    }

    #[test]
    fn legacy_incomplete_round_history_does_not_create_recent_absent_validators() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
        let legacy_round = set(8, RoundColor::Blue, vec!["bob"]);
        chain
            .rounds
            .insert(8, stored_round(&legacy_round, 100, false));

        let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

        assert!(absent.is_empty());
    }

    #[test]
    fn participation_matches_rotated_public_keys_by_wallet() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        chain.record_set(
            &ValidatorSetDto {
                validators: vec![validator_with_wallet("old-key", Some("-1:wallet"))],
                ..set(8, RoundColor::Blue, Vec::new())
            },
            100,
        );
        chain.record_set(
            &ValidatorSetDto {
                validators: vec![validator_with_wallet("new-key", Some("-1:wallet"))],
                ..set(10, RoundColor::Blue, Vec::new())
            },
            200,
        );

        let history = store.same_color_participation(
            "test",
            10,
            RoundColor::Blue,
            "new-key",
            Some("-1:wallet"),
        );

        assert!(matches!(
            history[3].status,
            ParticipationStatus::Participated
        ));
        assert!(matches!(
            history[4].status,
            ParticipationStatus::Participated
        ));
    }

    #[test]
    fn recent_absent_validators_uses_wallet_identity() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        chain.record_set(
            &ValidatorSetDto {
                validators: vec![validator_with_wallet("old-key", Some("-1:wallet"))],
                ..set(8, RoundColor::Blue, Vec::new())
            },
            100,
        );

        let current_validators = ValidatorIdentitySet::from_validators(&[validator_with_wallet(
            "new-key",
            Some("-1:wallet"),
        )]);
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

        assert!(absent.is_empty());
    }

    #[test]
    fn merge_ignores_legacy_incomplete_rounds() {
        let mut complete_store = RoundHistoryStore::default();
        complete_store
            .chains
            .entry("test".to_owned())
            .or_default()
            .record_set(&set(10, RoundColor::Blue, vec!["alice", "bob"]), 100);

        let mut legacy_store = RoundHistoryStore::default();
        let legacy_round = set(10, RoundColor::Blue, vec!["alice", "carol"]);
        legacy_store
            .chains
            .entry("test".to_owned())
            .or_default()
            .rounds
            .insert(10, stored_round(&legacy_round, 200, false));

        complete_store.merge_from(legacy_store);
        let round = &complete_store.chains["test"].rounds[&10];

        assert!(round.complete);
        assert!(round.validators.contains_key("alice"));
        assert!(round.validators.contains_key("bob"));
        assert!(!round.validators.contains_key("carol"));
    }

    #[test]
    fn merge_replaces_legacy_incomplete_round_with_complete_round() {
        let mut legacy_store = RoundHistoryStore::default();
        let legacy_round = set(10, RoundColor::Blue, vec!["alice"]);
        legacy_store
            .chains
            .entry("test".to_owned())
            .or_default()
            .rounds
            .insert(10, stored_round(&legacy_round, 100, false));

        let mut complete_store = RoundHistoryStore::default();
        complete_store
            .chains
            .entry("test".to_owned())
            .or_default()
            .record_set(&set(10, RoundColor::Blue, vec!["bob"]), 200);

        legacy_store.merge_from(complete_store);
        let round = &legacy_store.chains["test"].rounds[&10];

        assert!(round.complete);
        assert!(!round.validators.contains_key("alice"));
        assert!(round.validators.contains_key("bob"));
    }

    #[test]
    fn merge_keeps_newer_complete_round_authoritative() {
        let mut newer_store = RoundHistoryStore::default();
        newer_store
            .chains
            .entry("test".to_owned())
            .or_default()
            .record_set(
                &ValidatorSetDto {
                    validators: vec![validator_with_wallet("alice", Some("-1:wallet"))],
                    ..set(10, RoundColor::Blue, Vec::new())
                },
                200,
            );

        let mut older_store = RoundHistoryStore::default();
        older_store
            .chains
            .entry("test".to_owned())
            .or_default()
            .record_set(&set(10, RoundColor::Blue, vec!["bob"]), 100);

        assert!(!newer_store.merge_from(older_store));
        let round = &newer_store.chains["test"].rounds[&10];

        assert!(round.complete);
        assert!(round.validators.contains_key("alice"));
        assert!(!round.validators.contains_key("bob"));
        assert_eq!(
            round.validators["alice"].wallet.as_deref(),
            Some("-1:wallet")
        );
    }

    #[test]
    fn complete_round_refresh_preserves_existing_wallets() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        assert!(chain.record_set(
            &ValidatorSetDto {
                validators: vec![validator_with_wallet("alice", Some("-1:wallet"))],
                ..set(10, RoundColor::Blue, Vec::new())
            },
            100,
        ));

        assert!(!chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 200));
        let round = &chain.rounds[&10];

        assert_eq!(
            round.validators["alice"].wallet.as_deref(),
            Some("-1:wallet")
        );
        assert_eq!(round.observed_at, 100);
    }

    #[test]
    fn recording_same_complete_round_is_not_dirty() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();

        assert!(chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100));
        assert!(!chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 200));
    }

    #[test]
    fn retention_prunes_rounds_outside_visible_windows() {
        let mut store = RoundHistoryStore::default();
        let chain = store.chains.entry("test".to_owned()).or_default();
        for round_id in [10_u32, 12, 14, 16, 18, 20, 21] {
            let color = if round_id.is_multiple_of(2) {
                RoundColor::Blue
            } else {
                RoundColor::Green
            };
            chain.record_set(&set(round_id, color, vec!["alice"]), 100);
        }

        let mut retention = RoundHistoryRetention::default();
        retention.add_round_window("test", 20);
        assert!(store.prune_to_retention(&retention));

        let rounds = store.chains["test"]
            .rounds
            .keys()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(rounds, vec![12, 14, 16, 18, 20]);
    }

    #[test]
    fn round_history_lock_path_adds_lock_suffix() {
        assert_eq!(
            round_history_lock_path(Path::new("validators_clock_history.json")),
            PathBuf::from("validators_clock_history.json.lock")
        );
    }
}
