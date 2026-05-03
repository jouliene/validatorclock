use super::security::{
    add_security_headers, enforce_allowed_host, handle_options, json_error, query_forces_refresh,
    redirect_response,
};
use crate::chain::{chains_response, get_chain_snapshot, runtime_status};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::header::{self, HeaderValue};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::middleware;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceBuilder;
use tracing::error;

const INDEX_HTML: &str = include_str!("../../public/index.html");
const STYLES_CSS: &str = include_str!("../../public/styles.css");
const APP_JS: &str = include_str!("../../public/app.js");
const EVERSCALE_LOGO_SVG: &str = include_str!("../../public/brands/everscale.svg");
const TYCHO_LOGO_SVG: &str = include_str!("../../public/brands/tycho.svg");
const ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");

pub(super) fn app_router(state: Arc<AppState>) -> Router {
    let layers = ServiceBuilder::new()
        .layer(middleware::from_fn(add_security_headers))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            enforce_allowed_host,
        ))
        .layer(middleware::from_fn(handle_options));

    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/styles.css", get(styles))
        .route("/app.js", get(app_js))
        .route("/brands/everscale.svg", get(everscale_logo))
        .route("/brands/tycho.svg", get(tycho_logo))
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/chains", get(list_chains))
        .route("/api/chains/{chain_id}/clock", get(chain_clock))
        .fallback(not_found)
        .with_state(state)
        .layer(layers)
}

pub(super) fn challenge_redirect_router(state: Arc<AppState>) -> Router {
    let layers = ServiceBuilder::new()
        .layer(middleware::from_fn(add_security_headers))
        .layer(middleware::from_fn(handle_options));

    Router::new()
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .fallback(redirect_to_https)
        .with_state(state)
        .layer(layers)
}

async fn index() -> Html<String> {
    Html(INDEX_HTML.replace("__ASSET_VERSION__", &asset_version()))
}

pub(super) fn asset_version() -> String {
    format!(
        "{}-{:016x}",
        env!("CARGO_PKG_VERSION"),
        fnv1a64(&[
            INDEX_HTML,
            STYLES_CSS,
            APP_JS,
            EVERSCALE_LOGO_SVG,
            TYCHO_LOGO_SVG,
        ])
    )
}

fn fnv1a64(parts: &[&str]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

async fn styles() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/css; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        STYLES_CSS,
    )
}

async fn app_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        APP_JS,
    )
}

async fn everscale_logo() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        EVERSCALE_LOGO_SVG,
    )
}

async fn tycho_logo() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        TYCHO_LOGO_SVG,
    )
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn status(State(state): State<Arc<AppState>>) -> Response {
    match runtime_status(&state).await {
        Ok(status) => Json(status).into_response(),
        Err(error) => {
            error!(error = ?error, "status request failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "status_failed",
                "failed to build runtime status",
            )
        }
    }
}

async fn list_chains(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(chains_response(&state.config))
}

async fn chain_clock(
    State(state): State<Arc<AppState>>,
    Path(chain_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let force_refresh = state.config.security.allow_force_refresh && query_forces_refresh(&query);
    if state.config.chain(&chain_id).is_none() {
        return json_error(
            StatusCode::NOT_FOUND,
            "unknown_chain",
            &format!("unknown chain id `{chain_id}`"),
        );
    }

    match get_chain_snapshot(&state, &chain_id, force_refresh).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => {
            error!(chain_id, error = ?error, "snapshot request failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chain_snapshot_failed",
                "failed to fetch chain snapshot",
            )
        }
    }
}

async fn acme_challenge(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(value) = state.acme_challenges.read().await.get(&token).cloned() {
        return (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            value,
        )
            .into_response();
    }

    redirect_response(&state, &headers, &uri)
}

async fn redirect_to_https(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    redirect_response(&state, &headers, &uri)
}

async fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "not found")
}
