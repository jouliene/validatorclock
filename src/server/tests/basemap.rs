use super::*;
use axum::http::StatusCode;
use std::path::PathBuf;

// The archive is gigabytes. A plain GET of it used to read every byte into
// memory and then answer 200 with an empty body, because tagging gave up on a
// body that large and threw it away. These drive the whole router, so the
// handler and the tagging middleware are both on the path.
const LARGE_ARCHIVE_BYTES: u64 = 9 * 1024 * 1024;

#[tokio::test]
async fn a_large_archive_arrives_whole_instead_of_empty() {
    let dir = temp_basemap_dir("large");
    write_sparse(&dir.join("tiles.pmtiles"), LARGE_ARCHIVE_BYTES);
    let state = state_with_basemap(&dir);

    let response = app_response(state, "/basemap/tiles.pmtiles").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        content_length(&response),
        Some(LARGE_ARCHIVE_BYTES),
        "the archive should state its real length, not zero"
    );
    let response_headers = response.headers().clone();
    // Tagging a body means holding all of it, so an archive that was streamed
    // carries no entity tag. This is what tells a streamed answer from a
    // buffered one from the outside.
    assert!(
        !response_headers.contains_key(header::ETAG),
        "a large archive should be streamed, not buffered to be tagged"
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.len() as u64, LARGE_ARCHIVE_BYTES);

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_range_of_a_large_archive_is_served_from_its_offset() {
    let dir = temp_basemap_dir("range");
    write_sparse(&dir.join("tiles.pmtiles"), LARGE_ARCHIVE_BYTES);
    let state = state_with_basemap(&dir);

    let response = ranged_response(state, "/basemap/tiles.pmtiles", "bytes=10-19").await;

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
        Some(format!("bytes 10-19/{LARGE_ARCHIVE_BYTES}").as_str())
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.len(), 10);

    std::fs::remove_dir_all(&dir).ok();
}

/// Streaming the big archive must not cost the small assets their entity tag:
/// those are still buffered, so a repeat visit can be answered with a 304.
#[tokio::test]
async fn a_small_asset_still_carries_an_entity_tag() {
    let dir = temp_basemap_dir("small");
    std::fs::write(dir.join("sprite.png"), b"small enough to hash").unwrap();
    let state = state_with_basemap(&dir);

    let response = app_response(state, "/basemap/sprite.png").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().contains_key(header::ETAG),
        "a buffered asset should still be tagged"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The router percent-decodes a path parameter before the handler sees it, and
/// the handler used to decode a second time. A doubly-encoded separator came
/// back as a separator, the segment decoded to an absolute path, and
/// `PathBuf::push` threw the base directory away: any file the process could
/// open was served to anyone who asked, with no credentials.
#[tokio::test]
async fn a_doubly_encoded_path_cannot_escape_the_basemap_directory() {
    let dir = temp_basemap_dir("escape");
    std::fs::write(dir.join("style-ok.json"), b"{}").unwrap();
    let secret = dir.parent().unwrap().join("validatorclock_escape_secret");
    std::fs::write(&secret, b"tls-private-key").unwrap();

    for escape in [
        "%2fetc%2fhostname",
        "%252fetc%252fhostname",
        &format!(
            "%252f{}",
            secret
                .display()
                .to_string()
                .trim_start_matches('/')
                .replace('/', "%252f")
        ),
        "fonts%252f..%252f..%252fCargo.toml",
        "..%252f..%252fetc%252fpasswd",
    ] {
        let response = app_response(state_with_basemap(&dir), &format!("/basemap/{escape}")).await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{escape} should not resolve to a file"
        );
        assert!(body.is_empty(), "{escape} returned {} bytes", body.len());
    }

    std::fs::remove_file(&secret).ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// A client probing a footer asks for the last few bytes. The suffix form did
/// not parse, and an unparsed range is no range at all - so the answer was the
/// whole multi-gigabyte archive.
#[tokio::test]
async fn a_suffix_range_returns_the_end_of_the_file() {
    let dir = temp_basemap_dir("suffix");
    std::fs::write(dir.join("tiles.pmtiles"), b"0123456789").unwrap();
    let state = state_with_basemap(&dir);

    let response = ranged_response(state, "/basemap/tiles.pmtiles", "bytes=-4").await;

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
        Some("bytes 6-9/10")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"6789");

    std::fs::remove_dir_all(&dir).ok();
}

/// A range nothing can satisfy is a 416 with the real length, not the whole
/// file and not a 404.
#[tokio::test]
async fn a_range_past_the_end_is_answered_with_416() {
    let dir = temp_basemap_dir("unsatisfiable");
    std::fs::write(dir.join("tiles.pmtiles"), b"0123456789").unwrap();
    let state = state_with_basemap(&dir);

    let response = ranged_response(state, "/basemap/tiles.pmtiles", "bytes=99999-").await;

    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
        Some("bytes */10")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.is_empty(), "416 should not carry the file");

    std::fs::remove_dir_all(&dir).ok();
}

fn state_with_basemap(dir: &std::path::Path) -> std::sync::Arc<AppState> {
    let mut config = test_config(Vec::new());
    config.basemap_dir = Some(dir.to_path_buf());
    state_from_config(config)
}

fn write_sparse(path: &std::path::Path, length: u64) {
    let file = std::fs::File::create(path).unwrap();
    file.set_len(length).unwrap();
}

fn content_length(response: &axum::response::Response) -> Option<u64> {
    response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

async fn ranged_response(
    state: std::sync::Arc<AppState>,
    uri: &str,
    range: &str,
) -> axum::response::Response {
    crate::server::routes::app_router(state)
        .oneshot(
            axum::http::Request::builder()
                .uri(uri)
                .header(header::RANGE, range)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

fn temp_basemap_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "validatorclock_basemap_test_{name}_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
