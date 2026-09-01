//! Serves the basemap the dashboard draws under the validator nodes: one
//! pmtiles archive plus the fonts and sprite its style needs. Everything is
//! read from disk, so the map needs no third-party tile service and no key.

use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{self, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

// Tiles, fonts and the sprite never change under the same name; the style is
// edited by hand, so it is revalidated instead of pinned for a day.
const STATIC_CACHE_CONTROL: HeaderValue = HeaderValue::from_static("public, max-age=86400");
const STYLE_CACHE_CONTROL: HeaderValue = HeaderValue::from_static("no-cache");
const MAX_RANGE_BYTES: u64 = 8 * 1024 * 1024;

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

    match read_asset(&path, range) {
        Ok(asset) => asset.into_response(content_type_for(&path), cache_control),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                warn!(path = %path.display(), error = ?error, "failed to read a basemap asset");
                return StatusCode::NOT_FOUND.into_response();
            }
            // A missing glyph range answered with 404 takes down every layer
            // that shares the tile, so an alphabet nobody labelled here is
            // answered with no glyphs instead.
            if is_glyph_range(&asset_path) {
                return BasemapAsset {
                    bytes: Vec::new(),
                    range: None,
                }
                .into_response(content_type_for(&path), cache_control);
            }
            StatusCode::NOT_FOUND.into_response()
        }
    }
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
fn resolve_asset_path(base_dir: &std::path::Path, asset_path: &str) -> Option<PathBuf> {
    let mut path = base_dir.to_path_buf();
    for segment in asset_path.split('/') {
        let segment = percent_decode(segment);
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.starts_with('.')
            || segment.contains('\\')
        {
            return None;
        }
        path.push(segment);
    }
    Some(path)
}

fn percent_decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or_default();
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

struct BasemapAsset {
    bytes: Vec<u8>,
    range: Option<(u64, u64, u64)>,
}

impl BasemapAsset {
    fn into_response(self, content_type: &'static str, cache_control: HeaderValue) -> Response {
        let status = match self.range {
            Some(_) => StatusCode::PARTIAL_CONTENT,
            None => StatusCode::OK,
        };
        let mut response = (status, Body::from(self.bytes)).into_response();
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert(header::CACHE_CONTROL, cache_control);
        headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        if let Some((start, end, total)) = self.range
            && let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{total}"))
        {
            headers.insert(header::CONTENT_RANGE, value);
        }
        response
    }
}

fn read_asset(
    path: &std::path::Path,
    range: Option<(u64, Option<u64>)>,
) -> std::io::Result<BasemapAsset> {
    let mut file = std::fs::File::open(path)?;
    let total = file.metadata()?.len();

    let Some((start, end)) = range else {
        let mut bytes = Vec::with_capacity(total as usize);
        file.read_to_end(&mut bytes)?;
        return Ok(BasemapAsset { bytes, range: None });
    };

    if start >= total {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "range starts past the end of the file",
        ));
    }
    let end = end
        .unwrap_or(total - 1)
        .min(total - 1)
        .min(start + MAX_RANGE_BYTES - 1);
    let length = end - start + 1;

    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0u8; length as usize];
    file.read_exact(&mut bytes)?;

    Ok(BasemapAsset {
        bytes,
        range: Some((start, end, total)),
    })
}

/// Only the single-range form pmtiles uses.
fn parse_byte_range(value: &str) -> Option<(u64, Option<u64>)> {
    let spec = value.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (start, end) = spec.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = match end.trim() {
        "" => None,
        value => Some(value.parse::<u64>().ok()?),
    };
    if end.is_some_and(|end| end < start) {
        return None;
    }
    Some((start, end))
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
        assert_eq!(parse_byte_range("bytes=0-16383"), Some((0, Some(16383))));
        assert_eq!(parse_byte_range("bytes=128-"), Some((128, None)));
        assert_eq!(parse_byte_range(" bytes=5-9 "), Some((5, Some(9))));
        assert_eq!(parse_byte_range("bytes=9-5"), None);
        assert_eq!(parse_byte_range("bytes=0-1,4-5"), None);
        assert_eq!(parse_byte_range("items=0-1"), None);
    }

    #[test]
    fn asset_paths_stay_inside_the_basemap_directory() {
        let base = std::path::Path::new("/srv/basemap");

        assert_eq!(
            resolve_asset_path(base, "fonts/Noto%20Sans%20Regular/0-255.pbf"),
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
    }
}
