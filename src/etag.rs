//! The weak entity tag a body is served with.
//!
//! Two callers put one on: the middleware that tags whatever a handler
//! produced, and the pages that are written out ahead of time and carry their
//! tag with them.

/// Weak, because it says the bytes are the same, not that they were produced
/// the same way: a body compressed on the way out answers to the tag of the
/// body it was compressed from.
pub(crate) fn weak_entity_tag(body: &[u8]) -> String {
    let mut hash = Fnv1a64::new();
    hash.update(body);
    format!("W/\"{:016x}-{:x}\"", hash.finish(), body.len())
}

/// Whether an `If-None-Match` header offers the tag a body now has.
pub(crate) fn offered_tag_matches(offered: &str, entity_tag: &str) -> bool {
    offered.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            || candidate == entity_tag
            || candidate.trim_start_matches("W/") == entity_tag.trim_start_matches("W/")
    })
}

pub(crate) struct Fnv1a64 {
    value: u64,
}

impl Fnv1a64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    pub(crate) fn new() -> Self {
        Self {
            value: Self::OFFSET,
        }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u64::from(*byte);
            self.value = self.value.wrapping_mul(Self::PRIME);
        }
        self.value ^= 0xff;
        self.value = self.value.wrapping_mul(Self::PRIME);
    }

    pub(crate) fn finish(self) -> u64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_tags_are_weak_and_content_dependent() {
        let first = weak_entity_tag(b"snapshot");
        let second = weak_entity_tag(b"snapshot");
        let third = weak_entity_tag(b"snapshoT");

        assert!(first.starts_with("W/\""));
        assert_eq!(first, second);
        assert_ne!(first, third);
    }

    #[test]
    fn if_none_match_accepts_lists_wildcards_and_strength_changes() {
        let entity_tag = weak_entity_tag(b"snapshot");
        let strong = entity_tag.trim_start_matches("W/").to_owned();

        assert!(offered_tag_matches(&entity_tag, &entity_tag));
        assert!(offered_tag_matches(&strong, &entity_tag));
        assert!(offered_tag_matches("*", &entity_tag));
        assert!(offered_tag_matches(
            &format!("W/\"other\", {entity_tag}"),
            &entity_tag
        ));
        assert!(!offered_tag_matches("W/\"other\"", &entity_tag));
        assert!(!offered_tag_matches("", &entity_tag));
    }
}
