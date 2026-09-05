use crate::etag::offered_tag_matches;
use crate::state::RenderedJson;
use axum::Json;
use axum::body::Body;
use axum::http::StatusCode;
use axum::http::header::{self, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Serialize)]
struct ApiError<'a> {
    error: &'a str,
    code: &'a str,
}

pub(super) fn json_error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(ApiError {
            error: message,
            code,
        }),
    )
        .into_response()
}

pub(super) async fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "not found")
}

/// A page as it was written out when the snapshot behind it was built: the
/// same bytes for every reader, deflated once, under one entity tag.
pub(super) fn rendered_json(headers: &HeaderMap, rendered: &RenderedJson) -> Response {
    let Ok(entity_tag) = HeaderValue::from_str(&rendered.entity_tag) else {
        return json_body(Body::from(rendered.body.clone()), None, None);
    };

    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|offered| offered.to_str().ok())
        .is_some_and(|offered| offered_tag_matches(offered, &rendered.entity_tag));
    if unchanged {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        response.headers_mut().insert(header::ETAG, entity_tag);
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("accept-encoding"));
        return response;
    }

    match rendered.gzip.as_ref().filter(|_| accepts_gzip(headers)) {
        Some(gzip) => json_body(
            Body::from(gzip.clone()),
            Some(entity_tag),
            Some(HeaderValue::from_static("gzip")),
        ),
        None => json_body(Body::from(rendered.body.clone()), Some(entity_tag), None),
    }
}

fn json_body(
    body: Body,
    entity_tag: Option<HeaderValue>,
    content_encoding: Option<HeaderValue>,
) -> Response {
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Said whether or not this answer was compressed: the same URL returns
    // both, and a cache between here and the reader has to keep them apart.
    headers.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
    if let Some(entity_tag) = entity_tag {
        headers.insert(header::ETAG, entity_tag);
    }
    if let Some(content_encoding) = content_encoding {
        headers.insert(header::CONTENT_ENCODING, content_encoding);
    }
    response
}

fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(offers_gzip)
}

fn offers_gzip(encoding: &str) -> bool {
    let mut parts = encoding.split(';').map(str::trim);
    let named = parts.next().is_some_and(|name| {
        name.eq_ignore_ascii_case("gzip") || name == "*" || name.eq_ignore_ascii_case("x-gzip")
    });
    // `gzip;q=0` names it only to refuse it.
    named && !parts.any(refuses_it)
}

fn refuses_it(parameter: &str) -> bool {
    parameter
        .strip_prefix("q=")
        .and_then(|quality| quality.trim().parse::<f32>().ok())
        .is_some_and(|quality| quality <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accept_encoding(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_str(value).unwrap(),
        );
        headers
    }

    #[test]
    fn a_client_is_sent_the_compressed_copy_only_when_it_says_it_takes_one() {
        assert!(accepts_gzip(&accept_encoding("gzip")));
        assert!(accepts_gzip(&accept_encoding("gzip, deflate, br")));
        assert!(accepts_gzip(&accept_encoding("br;q=1.0, gzip;q=0.8")));
        assert!(accepts_gzip(&accept_encoding("*")));
        assert!(accepts_gzip(&accept_encoding("GZIP")));

        assert!(!accepts_gzip(&HeaderMap::new()));
        assert!(!accepts_gzip(&accept_encoding("br")));
        assert!(
            !accepts_gzip(&accept_encoding("gzip;q=0")),
            "naming an encoding with q=0 refuses it"
        );
        assert!(!accepts_gzip(&accept_encoding("br, gzip;q=0.0")));
    }
}
