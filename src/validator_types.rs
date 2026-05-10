use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

const KNOWN_CONTRACT_TYPES: &[(&str, &str)] = &[
    (
        "3ba6528ab2694c118180aa3bd10dd19ff400b909ab4dcf58fc69925b2c7b12a6",
        "EverWallet",
    ),
    (
        "435368efa80a95345edaec790c37957cd296fb0021071202fd90854641f5bb10",
        "StEverDePoolProxy",
    ),
    (
        "c05938cde3cee21141caacc9e88d3b8f2a4a4bc3968cb3d455d83cd0498d4375",
        "DePoolProxy",
    ),
    (
        "cc0d39589eb2c0cfe0fde28456657a3bdd3d953955ae3f98f25664ab3c904fbd",
        "SingleNominatorV1_1",
    ),
    (
        "a42ae69eac76ffe0e452d3d4f13d387a14e46c01a5aadba5fc1d893e6c71f5ba",
        "SingleNominatorV1_0",
    ),
    (
        "9a3ec14bc098f6b44064c305222caea2800f17dda85ee6a8198a7095ede10dcf",
        "TonNominatorPool",
    ),
    (
        "dd3ce98db487c7585803933bffba7a57eb4e663099059d08b83db0b4ce060793",
        "ValidatorController",
    ),
    (
        "13167d8f6618337ebdd0ede9a66ef7d767977e49841f04dc4165bab23eb1f1bc",
        "ValidatorController",
    ),
    (
        "3c62936b39cfe5a63ddfb206db60fca300d9fcabb8f17c068963071f0466125a",
        "ValidatorController",
    ),
    (
        "587cc789eff1c84f46ec3797e45fc809a14ff5ae24f1e0c7a6a99cc9dc9061ff",
        "TonWalletV1R3",
    ),
    (
        "b66c1630c39fa67f1daed236b52af3ce9e67544161b4373375e8b4eef1bcbc59",
        "TonVestingWallet",
    ),
    (
        "6097b64a3b9db526a6b26497afeae3f224a8f639d37b6534d46df21fe0589c21",
        "WhalesPoolProxy",
    ),
    (
        "8a025e9cd260112554c68d2c2f27fb41dd980b43e09cb34f0c678a89e6b7f4c7",
        "WhalesPoolProxy",
    ),
    (
        "c63a2766b55a96d54fd88c2727cee63feddfd3c42f5fe56983e3e436607eb2e5",
        "TonSingleNominatorPool",
    ),
    (
        "56fb96fc4b9051deecfce8b04ce3c888990ba80fe6bd07154e351506ee9907a0",
        "HipoValidatorProxy",
    ),
];

#[derive(Debug, Clone, Default)]
pub(crate) struct ValidatorTypeCache {
    entries: HashMap<String, ValidatorTypeCacheEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ValidatorTypeCacheEntry {
    pub(crate) repr_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<ValidatorSourceCacheEntry>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ValidatorSourceCacheEntry {
    pub(crate) address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) repr_hash: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ValidatorTypeDiskCache {
    version: u32,
    #[serde(default)]
    entries: HashMap<String, ValidatorTypeCacheEntry>,
}

impl ValidatorTypeCache {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn get(&self, chain_id: &str, wallet: &str) -> Option<&ValidatorTypeCacheEntry> {
        self.entries.get(&cache_key(chain_id, wallet))
    }

    pub(crate) fn insert(&mut self, chain_id: &str, wallet: &str, repr_hash: String) -> bool {
        let key = cache_key(chain_id, wallet);
        let entry = ValidatorTypeCacheEntry {
            repr_hash,
            source: self
                .entries
                .get(&key)
                .and_then(|current| current.source.clone()),
        };
        if self.entries.get(&key).is_some_and(|current| {
            current.repr_hash.eq_ignore_ascii_case(&entry.repr_hash)
                && current.source == entry.source
        }) {
            return false;
        }

        self.entries.insert(key, entry);
        true
    }

    pub(crate) fn insert_source(
        &mut self,
        chain_id: &str,
        wallet: &str,
        address: String,
        repr_hash: Option<String>,
    ) -> bool {
        let key = cache_key(chain_id, wallet);
        let Some(entry) = self.entries.get_mut(&key) else {
            return false;
        };
        let source = ValidatorSourceCacheEntry { address, repr_hash };
        if entry.source.as_ref() == Some(&source) {
            return false;
        }

        entry.source = Some(source);
        true
    }
}

fn cache_key(chain_id: &str, wallet: &str) -> String {
    format!("{chain_id}:{wallet}")
}

pub(crate) fn contract_type_name(repr_hash: &str) -> &'static str {
    KNOWN_CONTRACT_TYPES
        .iter()
        .find_map(|(hash, name)| repr_hash.eq_ignore_ascii_case(hash).then_some(*name))
        .unwrap_or("Unknown")
}

pub(crate) fn load_validator_type_cache(path: &Path) -> Result<ValidatorTypeCache> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(ValidatorTypeCache::default());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let disk: ValidatorTypeDiskCache = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(ValidatorTypeCache {
        entries: disk.entries,
    })
}

pub(crate) fn save_validator_type_cache(path: &Path, cache: &ValidatorTypeCache) -> Result<()> {
    let disk = ValidatorTypeDiskCache {
        version: 1,
        entries: cache.entries.clone(),
    };
    let content = serde_json::to_vec_pretty(&disk)?;
    write_file_atomic(path, &content, 0o644)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_contract_type_hashes() {
        assert_eq!(
            contract_type_name("3BA6528AB2694C118180AA3BD10DD19FF400B909AB4DCF58FC69925B2C7B12A6"),
            "EverWallet"
        );
        assert_eq!(
            contract_type_name("435368efa80a95345edaec790c37957cd296fb0021071202fd90854641f5bb10"),
            "StEverDePoolProxy"
        );
        assert_eq!(
            contract_type_name("c05938cde3cee21141caacc9e88d3b8f2a4a4bc3968cb3d455d83cd0498d4375"),
            "DePoolProxy"
        );
        assert_eq!(
            contract_type_name("cc0d39589eb2c0cfe0fde28456657a3bdd3d953955ae3f98f25664ab3c904fbd"),
            "SingleNominatorV1_1"
        );
        assert_eq!(
            contract_type_name("a42ae69eac76ffe0e452d3d4f13d387a14e46c01a5aadba5fc1d893e6c71f5ba"),
            "SingleNominatorV1_0"
        );
        assert_eq!(
            contract_type_name("9a3ec14bc098f6b44064c305222caea2800f17dda85ee6a8198a7095ede10dcf"),
            "TonNominatorPool"
        );
        assert_eq!(
            contract_type_name("dd3ce98db487c7585803933bffba7a57eb4e663099059d08b83db0b4ce060793"),
            "ValidatorController"
        );
        assert_eq!(
            contract_type_name("13167d8f6618337ebdd0ede9a66ef7d767977e49841f04dc4165bab23eb1f1bc"),
            "ValidatorController"
        );
        assert_eq!(
            contract_type_name("3c62936b39cfe5a63ddfb206db60fca300d9fcabb8f17c068963071f0466125a"),
            "ValidatorController"
        );
        assert_eq!(
            contract_type_name("587cc789eff1c84f46ec3797e45fc809a14ff5ae24f1e0c7a6a99cc9dc9061ff"),
            "TonWalletV1R3"
        );
        assert_eq!(
            contract_type_name("b66c1630c39fa67f1daed236b52af3ce9e67544161b4373375e8b4eef1bcbc59"),
            "TonVestingWallet"
        );
        assert_eq!(
            contract_type_name("6097b64a3b9db526a6b26497afeae3f224a8f639d37b6534d46df21fe0589c21"),
            "WhalesPoolProxy"
        );
        assert_eq!(
            contract_type_name("8a025e9cd260112554c68d2c2f27fb41dd980b43e09cb34f0c678a89e6b7f4c7"),
            "WhalesPoolProxy"
        );
        assert_eq!(
            contract_type_name("c63a2766b55a96d54fd88c2727cee63feddfd3c42f5fe56983e3e436607eb2e5"),
            "TonSingleNominatorPool"
        );
        assert_eq!(
            contract_type_name("56fb96fc4b9051deecfce8b04ce3c888990ba80fe6bd07154e351506ee9907a0"),
            "HipoValidatorProxy"
        );
    }

    #[test]
    fn maps_unknown_contract_type_hashes_to_unknown() {
        assert_eq!(
            contract_type_name("0ac45261b93f5679c35bc4d2d059a759b24709492bb5e3d143d92931356fc0bb"),
            "Unknown"
        );
    }
}
