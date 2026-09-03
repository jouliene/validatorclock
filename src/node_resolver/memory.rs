//! What the resolver already learned, so a slow lookup does not lose it.
//!
//! Each pass asks the DHT about every validator, and a handful time out every
//! time - not the same handful: of ten that failed one pass, five answered the
//! next, and one new one did not. Measured over eight passes the count moved
//! between 381 and 387 of 393, always with `lookup timed out`, never with
//! "no such address". The addresses are there. The budget is what runs out.
//!
//! So an address that answered before is kept and offered again when the
//! current pass cannot reach it. It is offered as what it is - remembered,
//! with the time it was last confirmed - and never as a fresh answer: this
//! project has just finished removing three places where old data was served
//! as current, and this is not a fourth.

use super::dht::{Resolution, ResolvedAddress};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// How long an address is worth offering after it was last confirmed. A
/// validator that moves is wrong for at most this long; one that is merely
/// slow to find is not lost at all.
const REMEMBER_FOR_SECONDS: u64 = 3_600;

#[derive(Debug, Default)]
pub(super) struct ResolvedAddressMemory {
    by_adnl: HashMap<String, Remembered>,
}

#[derive(Debug, Clone)]
struct Remembered {
    address: ResolvedAddress,
    confirmed_at: u64,
}

impl ResolvedAddressMemory {
    /// Pick up what the last run left behind.
    ///
    /// The resolver writes its answers to a file, which is also what the map
    /// reads. Reading it back at startup means a restart does not begin by
    /// forgetting - the site is restarted often while it is being worked on.
    pub(super) fn from_previous_output(path: &Path) -> Self {
        let Ok(body) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(previous) = serde_json::from_str::<PreviousOutput>(&body) else {
            return Self::default();
        };

        let mut by_adnl = HashMap::new();
        for validator in previous.validators {
            let (Some(adnl_addr), Some(address)) = (
                validator.adnl_addr,
                validator.resolution.addresses.into_iter().next(),
            ) else {
                continue;
            };
            let confirmed_at = validator
                .resolution
                .confirmed_at
                .unwrap_or(previous.generated_at);
            by_adnl.insert(
                adnl_addr,
                Remembered {
                    address,
                    confirmed_at,
                },
            );
        }
        Self { by_adnl }
    }

    pub(super) fn len(&self) -> usize {
        self.by_adnl.len()
    }

    /// Record an address the DHT has just confirmed.
    pub(super) fn remember(&mut self, adnl_addr: &str, address: &ResolvedAddress, now: u64) {
        self.by_adnl.insert(
            adnl_addr.to_owned(),
            Remembered {
                address: address.clone(),
                confirmed_at: now,
            },
        );
    }

    /// What is known about an address the current pass could not reach.
    pub(super) fn recall(&self, adnl_addr: &str, now: u64) -> Option<Resolution> {
        let remembered = self.by_adnl.get(adnl_addr)?;
        let age = now.saturating_sub(remembered.confirmed_at);
        (age <= REMEMBER_FOR_SECONDS)
            .then(|| Resolution::remembered(remembered.address.clone(), remembered.confirmed_at))
    }

    /// Forget validators that are no longer in the set, so a chain that has
    /// rotated its whole membership does not carry the old one forever.
    pub(super) fn retain_only(&mut self, keep: &[String]) {
        let keep: std::collections::HashSet<&str> = keep.iter().map(String::as_str).collect();
        self.by_adnl
            .retain(|adnl_addr, _| keep.contains(adnl_addr.as_str()));
    }
}

/// Just enough of the file this writes to read the addresses back out of it.
#[derive(Debug, Deserialize)]
struct PreviousOutput {
    #[serde(default)]
    generated_at: u64,
    #[serde(default)]
    validators: Vec<PreviousValidator>,
}

#[derive(Debug, Deserialize)]
struct PreviousValidator {
    #[serde(default)]
    adnl_addr: Option<String>,
    #[serde(default)]
    resolution: PreviousResolution,
}

#[derive(Debug, Default, Deserialize)]
struct PreviousResolution {
    #[serde(default)]
    addresses: Vec<ResolvedAddress>,
    #[serde(default)]
    confirmed_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(ip: &str) -> ResolvedAddress {
        ResolvedAddress {
            ip: ip.to_owned(),
            port: 30303,
            version: "udp4".to_owned(),
        }
    }

    #[test]
    fn an_address_confirmed_recently_is_offered_when_the_lookup_times_out() {
        let mut memory = ResolvedAddressMemory::default();
        memory.remember("aa", &address("104.238.222.200"), 1_000);

        let recalled = memory.recall("aa", 1_600).expect("still worth offering");
        assert_eq!(recalled.status, "remembered");
        assert_eq!(recalled.addresses[0].ip, "104.238.222.200");
        assert_eq!(
            recalled.confirmed_at,
            Some(1_000),
            "when it was last confirmed is part of the answer, not a footnote"
        );
    }

    #[test]
    fn an_address_no_one_has_confirmed_in_an_hour_is_let_go() {
        let mut memory = ResolvedAddressMemory::default();
        memory.remember("aa", &address("104.238.222.200"), 1_000);

        assert!(memory.recall("aa", 1_000 + REMEMBER_FOR_SECONDS).is_some());
        assert!(
            memory
                .recall("aa", 1_000 + REMEMBER_FOR_SECONDS + 1)
                .is_none(),
            "a validator that moved should not be reported at its old address for ever"
        );
    }

    #[test]
    fn nothing_is_offered_for_a_validator_never_seen() {
        let memory = ResolvedAddressMemory::default();
        assert!(memory.recall("aa", 1_000).is_none());
    }

    #[test]
    fn validators_that_have_left_the_set_are_forgotten() {
        let mut memory = ResolvedAddressMemory::default();
        memory.remember("aa", &address("1.1.1.1"), 1_000);
        memory.remember("bb", &address("2.2.2.2"), 1_000);

        memory.retain_only(&["aa".to_owned()]);

        assert_eq!(memory.len(), 1);
        assert!(memory.recall("aa", 1_000).is_some());
        assert!(memory.recall("bb", 1_000).is_none());
    }

    #[test]
    fn the_file_the_last_run_left_is_read_back() {
        let dir = std::env::temp_dir().join(format!("nr-memory-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.json");
        std::fs::write(
            &path,
            r#"{"generated_at": 5000, "validators": [
                {"adnl_addr": "aa", "resolution": {"status": "resolved",
                 "addresses": [{"ip": "9.9.9.9", "port": 1, "version": "udp4"}]}},
                {"adnl_addr": "bb", "resolution": {"status": "failed", "addresses": []}}
            ]}"#,
        )
        .unwrap();

        let memory = ResolvedAddressMemory::from_previous_output(&path);
        assert_eq!(
            memory.len(),
            1,
            "only the ones with an address are worth keeping"
        );
        assert_eq!(
            memory.recall("aa", 5_100).unwrap().addresses[0].ip,
            "9.9.9.9"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_or_unreadable_file_is_simply_no_memory() {
        assert_eq!(
            ResolvedAddressMemory::from_previous_output(Path::new("/nonexistent/x.json")).len(),
            0
        );
    }
}
