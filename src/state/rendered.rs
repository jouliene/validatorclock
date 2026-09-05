//! A chain's answer, written out once for everyone who asks for it.
//!
//! The clock is the largest thing this server sends - a megabyte for TON -
//! and it changes once a refresh, not once a request. Writing it out per
//! request meant three passes over that megabyte every time: serde to produce
//! it, the hash behind its entity tag, and the deflate on the way out. All
//! three now happen where the snapshot is built.

use crate::chain::ClockSnapshot;
use crate::etag::weak_entity_tag;
use bytes::Bytes;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write;
use tracing::warn;

/// Below this a deflate pass is not worth its own header, which is the size
/// the compression layer would have applied it from anyway.
const MIN_COMPRESSED_BYTES: usize = 32;

pub(crate) struct RenderedClock {
    /// The JSON exactly as `serde_json` writes it.
    pub(crate) body: Bytes,
    /// The same bytes deflated, for the clients that say they take it.
    pub(crate) gzip: Option<Bytes>,
    /// Weak, and taken over `body`: a client revalidating gets the same
    /// answer whichever of the two encodings it was served.
    pub(crate) entity_tag: String,
}

impl RenderedClock {
    pub(super) fn of(snapshot: &ClockSnapshot) -> Option<Self> {
        let body = match serde_json::to_vec(snapshot) {
            Ok(body) => Bytes::from(body),
            Err(error) => {
                warn!(error = ?error, "failed to write out a chain snapshot");
                return None;
            }
        };
        Some(Self {
            entity_tag: weak_entity_tag(&body),
            gzip: gzip(&body),
            body,
        })
    }
}

fn gzip(body: &[u8]) -> Option<Bytes> {
    if body.len() < MIN_COMPRESSED_BYTES {
        return None;
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    // A body that cannot be compressed is served as it is; the client asked
    // for less to carry, not for nothing.
    encoder
        .write_all(body)
        .and_then(|()| encoder.finish())
        .map(Bytes::from)
        .inspect_err(|error| warn!(error = ?error, "failed to compress a chain snapshot"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::test_clock_snapshot;
    use std::io::Read;

    fn snapshot() -> ClockSnapshot {
        test_clock_snapshot("test")
    }

    #[test]
    fn the_body_is_what_serde_would_have_written_and_the_tag_covers_it() {
        let snapshot = snapshot();
        let rendered = RenderedClock::of(&snapshot).expect("a snapshot writes out");

        assert_eq!(
            rendered.body.as_ref(),
            serde_json::to_vec(&snapshot).unwrap(),
            "readers must get the bytes they would have got serialized per request"
        );
        assert_eq!(rendered.entity_tag, weak_entity_tag(&rendered.body));
    }

    #[test]
    fn the_compressed_copy_says_the_same_thing() {
        let snapshot = snapshot();
        let rendered = RenderedClock::of(&snapshot).expect("a snapshot writes out");
        let gzip = rendered.gzip.expect("a snapshot is worth compressing");

        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(gzip.as_ref())
            .read_to_end(&mut decoded)
            .expect("what we send back deflates");
        assert_eq!(decoded, rendered.body.as_ref());
    }

    #[test]
    fn a_body_too_small_to_be_worth_deflating_is_left_alone() {
        assert!(gzip(b"{}").is_none());
    }
}
