use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::header::{self, HeaderValue};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

const MAX_TAGGED_BODY_BYTES: usize = 32 * 1024 * 1024;

pub(super) async fn add_entity_tags(request: Request, next: Next) -> Response {
    let tagged_method = request.method() == Method::GET;
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let response = next.run(request).await;
    if !tagged_method || !response_can_be_tagged(&response) {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let Ok(body) = to_bytes(body, MAX_TAGGED_BODY_BYTES).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let Ok(entity_tag) = HeaderValue::from_str(&weak_entity_tag(&body)) else {
        return Response::from_parts(parts, Body::from(body));
    };

    if if_none_match.is_some_and(|offered| matches_entity_tag(&offered, &entity_tag)) {
        parts.status = StatusCode::NOT_MODIFIED;
        parts.headers.remove(header::CONTENT_LENGTH);
        parts.headers.remove(header::CONTENT_TYPE);
        parts.headers.insert(header::ETAG, entity_tag);
        return Response::from_parts(parts, Body::empty());
    }

    parts.headers.insert(header::ETAG, entity_tag);
    Response::from_parts(parts, Body::from(body))
}

fn response_can_be_tagged(response: &Response) -> bool {
    if response.status() != StatusCode::OK || response.headers().contains_key(header::ETAG) {
        return false;
    }
    if response.headers().contains_key(header::CONTENT_ENCODING) {
        return false;
    }

    // Private responses must not be revalidated from a stored copy, so they are
    // served in full every time.
    !response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("no-store"))
}

fn weak_entity_tag(body: &[u8]) -> String {
    let mut hash = Fnv1a64::new();
    hash.update(body);
    format!("W/\"{:016x}-{:x}\"", hash.finish(), body.len())
}

fn matches_entity_tag(offered: &str, entity_tag: &HeaderValue) -> bool {
    let Ok(entity_tag) = entity_tag.to_str() else {
        return false;
    };
    offered.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            || candidate == entity_tag
            || candidate.trim_start_matches("W/") == entity_tag.trim_start_matches("W/")
    })
}

pub(super) struct Fnv1a64 {
    value: u64,
}

impl Fnv1a64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    pub(super) fn new() -> Self {
        Self {
            value: Self::OFFSET,
        }
    }

    pub(super) fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u64::from(*byte);
            self.value = self.value.wrapping_mul(Self::PRIME);
        }
        self.value ^= 0xff;
        self.value = self.value.wrapping_mul(Self::PRIME);
    }

    pub(super) fn finish(self) -> u64 {
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
        let entity_tag = HeaderValue::from_str(&weak_entity_tag(b"snapshot")).unwrap();
        let tag = entity_tag.to_str().unwrap().to_owned();
        let strong = tag.trim_start_matches("W/").to_owned();

        assert!(matches_entity_tag(&tag, &entity_tag));
        assert!(matches_entity_tag(&strong, &entity_tag));
        assert!(matches_entity_tag("*", &entity_tag));
        assert!(matches_entity_tag(
            &format!("W/\"other\", {tag}"),
            &entity_tag
        ));
        assert!(!matches_entity_tag("W/\"other\"", &entity_tag));
        assert!(!matches_entity_tag("", &entity_tag));
    }
}
