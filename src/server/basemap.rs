//! Serves the basemap the dashboard draws under the validator nodes: one
//! pmtiles archive plus the fonts and sprite its style needs. Everything is
//! read from disk, so the map needs no third-party tile service and no key.

use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{self, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use std::io::{Read, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use tracing::debug;

// Tiles, fonts and the sprite never change under the same name; the style is
// edited by hand, so it is revalidated instead of pinned for a day.
const STATIC_CACHE_CONTROL: HeaderValue = HeaderValue::from_static("public, max-age=86400");
const STYLE_CACHE_CONTROL: HeaderValue = HeaderValue::from_static("no-cache");
const MAX_RANGE_BYTES: u64 = 8 * 1024 * 1024;
// Anything larger is streamed rather than read into memory. One request must
// never cost as much memory as the file it names: the tile archive weighs
// gigabytes, and a plain GET of it used to allocate all of them at once.
const MAX_BUFFERED_BYTES: u64 = 8 * 1024 * 1024;

pub(super) async fn basemap_asset(
    State(state): State<Arc<AppState>>,
    Path(asset_path): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(base_dir) = state.config.basemap_dir.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // The style ships with the binary so it always matches the code, and its
    // zoom range is taken from the installed archive so the two cannot drift.
    if asset_path == "style.json" {
        return basemap_style(&base_dir);
    }
    let Some(path) = resolve_asset_path(&base_dir, &asset_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_byte_range);

    let cache_control = if path.extension().and_then(|value| value.to_str()) == Some("json") {
        STYLE_CACHE_CONTROL
    } else {
        STATIC_CACHE_CONTROL
    };

    match read_asset(&path, range).await {
        Ok(asset) => asset.into_response(content_type_for(&path), cache_control),
        Err(AssetError::Unsatisfiable { total }) => unsatisfiable_range(total),
        Err(AssetError::Unreadable(error)) => {
            // A caller chooses the path, so this is reachable by asking for a
            // directory. At warn it wrote a line per request into the journal,
            // where it can crowd out what an operator actually needs to see.
            debug!(path = %path.display(), error = ?error, "failed to read a basemap asset");
            StatusCode::NOT_FOUND.into_response()
        }
        Err(AssetError::Missing) => {
            // A missing glyph range answered with 404 takes down every layer
            // that shares the tile, so an alphabet nobody labelled here is
            // answered with no glyphs instead.
            if is_glyph_range(&asset_path) {
                return BasemapAsset {
                    body: Body::empty(),
                    length: 0,
                    range: None,
                }
                .into_response(content_type_for(&path), cache_control);
            }
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

/// RFC 9110: a range nothing can satisfy is answered with 416 and the real
/// length, so the client can ask again. Answering the whole file instead sent
/// gigabytes to a client that asked for a few hundred bytes.
fn unsatisfiable_range(total: u64) -> Response {
    let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    if let Ok(value) = HeaderValue::from_str(&format!("bytes */{total}")) {
        response.headers_mut().insert(header::CONTENT_RANGE, value);
    }
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response
}

fn basemap_style(base_dir: &std::path::Path) -> Response {
    static STYLE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let style = STYLE.get_or_init(|| {
        let max_zoom = archive_max_zoom(&base_dir.join("tiles.pmtiles"));
        style_with_max_zoom(super::assets::BASEMAP_STYLE_JSON, max_zoom)
    });

    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, STYLE_CACHE_CONTROL),
        ],
        style.clone(),
    )
        .into_response()
}

/// A pmtiles v3 header carries the zoom range at fixed offsets, so the style
/// can describe exactly the archive that is installed.
fn archive_max_zoom(path: &std::path::Path) -> Option<u8> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 128];
    file.read_exact(&mut header).ok()?;
    (&header[..7] == b"PMTiles").then_some(header[101])
}

fn style_with_max_zoom(style: &str, max_zoom: Option<u8>) -> String {
    let Some(max_zoom) = max_zoom else {
        return style.to_owned();
    };
    let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(style) else {
        return style.to_owned();
    };
    if let Some(source) = parsed
        .get_mut("sources")
        .and_then(|sources| sources.get_mut("protomaps"))
        .and_then(|source| source.as_object_mut())
    {
        source.insert("maxzoom".to_owned(), serde_json::json!(max_zoom));
    }
    serde_json::to_string(&parsed).unwrap_or_else(|_| style.to_owned())
}

fn is_glyph_range(asset_path: &str) -> bool {
    asset_path.starts_with("fonts/") && asset_path.ends_with(".pbf")
}

/// Keeps the request inside the basemap directory: no absolute paths, no
/// parent hops, no hidden files.
///
/// The router has already percent-decoded the path by the time it arrives, so
/// nothing is decoded here. Decoding a second time turned a doubly-encoded
/// %252f back into a separator, and a segment that decoded to an absolute path
/// replaced the whole base directory - `PathBuf::push` does that - so every
/// file the process could read was served to anyone who asked.
fn resolve_asset_path(base_dir: &std::path::Path, asset_path: &str) -> Option<PathBuf> {
    let mut path = base_dir.to_path_buf();
    for segment in asset_path.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.starts_with('.')
            || segment.contains('\\')
            || std::path::Path::new(segment).is_absolute()
        {
            return None;
        }
        path.push(segment);
    }

    // A last check on the joined path, so a segment nobody thought of cannot
    // walk out of the directory this is allowed to serve.
    path.starts_with(base_dir).then_some(path)
}

struct BasemapAsset {
    body: Body,
    length: u64,
    range: Option<(u64, u64, u64)>,
}

impl BasemapAsset {
    fn into_response(self, content_type: &'static str, cache_control: HeaderValue) -> Response {
        let status = match self.range {
            Some(_) => StatusCode::PARTIAL_CONTENT,
            None => StatusCode::OK,
        };
        let mut response = (status, self.body).into_response();
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert(header::CACHE_CONTROL, cache_control);
        headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        // A streamed body carries no length of its own, so it is stated here.
        headers.insert(header::CONTENT_LENGTH, self.length.into());
        if let Some((start, end, total)) = self.range
            && let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{total}"))
        {
            headers.insert(header::CONTENT_RANGE, value);
        }
        response
    }
}

/// Reads off the runtime's blocking-free path: the handler is async, so the
/// file is opened and read through tokio rather than blocking a worker thread
/// for as long as the disk takes.
/// What went wrong reading an asset, so the handler can answer each case the
/// way the specification asks rather than turning them all into 404.
enum AssetError {
    Missing,
    /// Nothing in the file can satisfy the range that was asked for.
    Unsatisfiable {
        total: u64,
    },
    Unreadable(std::io::Error),
}

async fn read_asset(
    path: &std::path::Path,
    range: Option<ByteRange>,
) -> Result<BasemapAsset, AssetError> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AssetError::Missing
        } else {
            AssetError::Unreadable(error)
        }
    })?;
    let total = file.metadata().await.map_err(AssetError::Unreadable)?.len();

    let Some(range) = range else {
        // A small file is buffered, so it can still carry an entity tag. A
        // large one is streamed: the response then costs a buffer, not the
        // whole archive.
        if total > MAX_BUFFERED_BYTES {
            return Ok(BasemapAsset {
                body: Body::from_stream(ReaderStream::new(file)),
                length: total,
                range: None,
            });
        }
        let mut bytes = Vec::with_capacity(total as usize);
        file.read_to_end(&mut bytes)
            .await
            .map_err(AssetError::Unreadable)?;
        return Ok(BasemapAsset {
            length: bytes.len() as u64,
            body: Body::from(bytes),
            range: None,
        });
    };

    let Some((start, end)) = range.resolve(total) else {
        return Err(AssetError::Unsatisfiable { total });
    };
    let length = end - start + 1;

    // Streamed like the whole file: a range is capped at a few megabytes, but
    // one buffer per connection still adds up to gigabytes across the
    // connection limit, held for as long as a client declines to read.
    file.seek(SeekFrom::Start(start))
        .await
        .map_err(AssetError::Unreadable)?;

    Ok(BasemapAsset {
        body: Body::from_stream(ReaderStream::new(file.take(length))),
        length,
        range: Some((start, end, total)),
    })
}

/// The single-range forms of RFC 9110: `bytes=start-end`, `bytes=start-`, and
/// the suffix `bytes=-last`. The suffix form used to fail to parse, and an
/// unparsed range is treated as no range at all - so a client asking for the
/// last few bytes of the archive was handed the whole of it.
fn parse_byte_range(value: &str) -> Option<ByteRange> {
    let spec = value.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (start, end) = spec.split_once('-')?;
    let (start, end) = (start.trim(), end.trim());

    if start.is_empty() {
        return Some(ByteRange::Suffix(end.parse::<u64>().ok()?));
    }

    let start = start.parse::<u64>().ok()?;
    let end = match end {
        "" => None,
        value => Some(value.parse::<u64>().ok()?),
    };
    if end.is_some_and(|end| end < start) {
        return None;
    }
    Some(ByteRange::From { start, end })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteRange {
    From {
        start: u64,
        end: Option<u64>,
    },
    /// The last N bytes, which is how a client probes a footer.
    Suffix(u64),
}

impl ByteRange {
    /// Resolves against the real length, or reports that nothing can satisfy
    /// it - which RFC 9110 answers with 416, not with the whole file.
    fn resolve(self, total: u64) -> Option<(u64, u64)> {
        let (start, end) = match self {
            ByteRange::From { start, end } => {
                if start >= total {
                    return None;
                }
                (start, end.unwrap_or(total - 1).min(total - 1))
            }
            ByteRange::Suffix(last) => {
                if last == 0 || total == 0 {
                    return None;
                }
                (total.saturating_sub(last), total - 1)
            }
        };
        Some((start, end.min(start + MAX_RANGE_BYTES - 1)))
    }
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("json") => "application/json; charset=utf-8",
        Some("pbf") => "application/x-protobuf",
        Some("png") => "image/png",
        Some("pmtiles") => "application/octet-stream",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_style_takes_its_zoom_range_from_the_archive() {
        let style = r#"{"sources":{"protomaps":{"type":"vector","maxzoom":10}}}"#;

        let patched = style_with_max_zoom(style, Some(8));
        let parsed: serde_json::Value = serde_json::from_str(&patched).unwrap();

        assert_eq!(parsed["sources"]["protomaps"]["maxzoom"], 8);
        assert_eq!(style_with_max_zoom(style, None), style);
        assert_eq!(style_with_max_zoom("not json", Some(8)), "not json");
    }

    /// MapLibre refuses a relative sprite URL and drops the whole style, and a
    /// layer that names an icon without a sprite logs a missing image on every
    /// load. Both went unnoticed once, so the style is checked here.
    #[test]
    fn the_style_names_no_icon_it_cannot_load() {
        let style: serde_json::Value =
            serde_json::from_str(super::super::assets::BASEMAP_STYLE_JSON).unwrap();

        let sprite = style.get("sprite").and_then(|value| value.as_str());
        assert!(
            sprite.is_none_or(|url| url.starts_with("http")),
            "a sprite URL must be absolute, got {sprite:?}"
        );

        if sprite.is_none() {
            let with_icons = style["layers"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|layer| layer.pointer("/layout/icon-image").is_some())
                .map(|layer| layer["id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>();
            assert!(
                with_icons.is_empty(),
                "these layers ask for an icon but the style has no sprite: {with_icons:?}"
            );
        }
    }

    #[test]
    fn glyph_ranges_are_recognised() {
        assert!(is_glyph_range("fonts/Noto Sans Medium/0-255.pbf"));
        assert!(is_glyph_range("fonts/Noto%20Sans%20Medium/5120-5375.pbf"));
        assert!(!is_glyph_range("style.json"));
        assert!(!is_glyph_range("tiles.pmtiles"));
        assert!(!is_glyph_range("sprite/dark.png"));
    }

    #[test]
    fn ranges_are_parsed_in_the_form_pmtiles_sends() {
        assert_eq!(
            parse_byte_range("bytes=0-16383"),
            Some(ByteRange::From {
                start: 0,
                end: Some(16383)
            })
        );
        assert_eq!(
            parse_byte_range("bytes=128-"),
            Some(ByteRange::From {
                start: 128,
                end: None
            })
        );
        assert_eq!(
            parse_byte_range(" bytes=5-9 "),
            Some(ByteRange::From {
                start: 5,
                end: Some(9)
            })
        );
        assert_eq!(parse_byte_range("bytes=9-5"), None);
        assert_eq!(parse_byte_range("bytes=0-1,4-5"), None);
        assert_eq!(parse_byte_range("items=0-1"), None);

        // The suffix form. It used to fail to parse, and an unparsed range is
        // no range at all - so a client asking for the last 500 bytes of a
        // multi-gigabyte archive was handed the whole of it.
        assert_eq!(parse_byte_range("bytes=-500"), Some(ByteRange::Suffix(500)));

        assert_eq!(ByteRange::Suffix(500).resolve(2_000), Some((1_500, 1_999)));
        assert_eq!(ByteRange::Suffix(9_000).resolve(2_000), Some((0, 1_999)));
        assert_eq!(ByteRange::Suffix(0).resolve(2_000), None);
        assert_eq!(
            ByteRange::From {
                start: 2_000,
                end: None
            }
            .resolve(2_000),
            None,
            "a range past the end can satisfy nothing"
        );
    }

    #[test]
    fn asset_paths_stay_inside_the_basemap_directory() {
        let base = std::path::Path::new("/srv/basemap");

        // The router decodes before the handler sees it, so a space arrives
        // as a space.
        assert_eq!(
            resolve_asset_path(base, "fonts/Noto Sans Regular/0-255.pbf"),
            Some(PathBuf::from(
                "/srv/basemap/fonts/Noto Sans Regular/0-255.pbf"
            ))
        );
        assert_eq!(
            resolve_asset_path(base, "style.json"),
            Some(PathBuf::from("/srv/basemap/style.json"))
        );
        assert_eq!(resolve_asset_path(base, "../../etc/passwd"), None);
        assert_eq!(resolve_asset_path(base, "fonts/../../secret"), None);
        assert_eq!(resolve_asset_path(base, ".ssh/id_rsa"), None);
        assert_eq!(resolve_asset_path(base, "a//b"), None);

        // What a second round of decoding used to produce. `PathBuf::push`
        // replaces the whole buffer when handed an absolute path, so this was
        // an unauthenticated read of any file the process could open.
        assert_eq!(resolve_asset_path(base, "/etc/passwd"), None);
        assert_eq!(resolve_asset_path(base, "/home/admin/.ssh/id_rsa"), None);
    }
}
