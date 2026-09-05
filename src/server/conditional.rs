use crate::etag::{offered_tag_matches, weak_entity_tag};
use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::header::{self, HeaderValue};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use hyper::body::Body as HttpBody;
use tracing::warn;

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
    // Tagging means hashing, and hashing means holding the whole body. A body
    // that does not declare a size that fits is handed on untouched: buffering
    // it would cost as much memory as the body weighs, and a buffer that gave
    // up used to answer with an empty 200 instead of the file.
    if !fits_in_memory(&body) {
        return Response::from_parts(parts, body);
    }
    let Ok(body) = to_bytes(body, MAX_TAGGED_BODY_BYTES).await else {
        warn!("failed to read a response body while tagging it");
        return Response::from_parts(parts, Body::empty());
    };
    let Ok(entity_tag) = HeaderValue::from_str(&weak_entity_tag(&body)) else {
        return Response::from_parts(parts, Body::from(body));
    };

    if if_none_match.is_some_and(|offered| {
        entity_tag
            .to_str()
            .is_ok_and(|tag| offered_tag_matches(&offered, tag))
    }) {
        parts.status = StatusCode::NOT_MODIFIED;
        parts.headers.remove(header::CONTENT_LENGTH);
        parts.headers.remove(header::CONTENT_TYPE);
        parts.headers.insert(header::ETAG, entity_tag);
        return Response::from_parts(parts, Body::empty());
    }

    parts.headers.insert(header::ETAG, entity_tag);
    Response::from_parts(parts, Body::from(body))
}

/// Only a body whose exact size is known and small enough is worth buffering.
/// A streamed body reports no exact size, so it is left alone.
fn fits_in_memory(body: &Body) -> bool {
    body.size_hint()
        .exact()
        .is_some_and(|size| size <= MAX_TAGGED_BODY_BYTES as u64)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Buffering a body to hash it costs as much memory as the body weighs, so
    /// a body that does not declare a size that fits is left alone. It used to
    /// be swallowed instead, and the client got an empty 200.
    #[test]
    fn a_body_that_does_not_fit_is_left_alone() {
        assert!(fits_in_memory(&Body::from(vec![0u8; 64])));
        assert!(!fits_in_memory(&Body::from(vec![
            0u8;
            MAX_TAGGED_BODY_BYTES + 1
        ])));
        assert!(
            !fits_in_memory(&Body::from_stream(tokio_util::io::ReaderStream::new(
                tokio::io::empty()
            ))),
            "a streamed body states no exact size and must not be buffered"
        );
    }
}
