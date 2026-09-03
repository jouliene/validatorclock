use super::acme::{acme_challenge, redirect_to_https};
use super::api::{
    analytics_event, chain_clock, chain_map, chain_round_stats, health, list_chains,
    public_analytics, public_visitors, status,
};
use super::assets::{
    app_js, everscale_logo, index, jokes_json, maplibre_css, maplibre_js, pmtiles_js,
    portrait_image, smoking_man_png, stats_js, stats_page, styles, ton_logo, tycho_logo,
};
use super::basemap::basemap_asset;
use super::conditional::add_entity_tags;
use super::responses::not_found;
use super::security::{
    add_security_headers, enforce_allowed_host, handle_options, require_stats_auth,
};
use crate::state::AppState;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

pub(super) fn app_router(state: Arc<AppState>) -> Router {
    // The tile archive is already compressed inside, and it is gigabytes: a
    // deflate pass over it saves nothing and costs a runtime worker for as
    // long as the transfer lasts. Nothing else this server sends is
    // octet-stream, so excluding it costs no real compression.
    let compression = CompressionLayer::new().compress_when(
        DefaultPredicate::new().and(NotForContentType::new("application/octet-stream")),
    );

    let layers = ServiceBuilder::new()
        .layer(compression)
        .layer(middleware::from_fn(add_entity_tags))
        .layer(middleware::from_fn(add_security_headers))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            enforce_allowed_host,
        ))
        .layer(middleware::from_fn(handle_options));

    Router::new()
        .merge(stats_router(Arc::clone(&state)))
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/styles.css", get(styles))
        .route("/app.js", get(app_js))
        .route("/basemap/{*path}", get(basemap_asset))
        // Named for the upstream version: these are served immutable for a
        // year, so a new version has to arrive at a new URL.
        .route("/vendor/maplibre-gl-5.9.0.js", get(maplibre_js))
        .route("/vendor/maplibre-gl-5.9.0.css", get(maplibre_css))
        .route("/vendor/pmtiles-4.3.0.js", get(pmtiles_js))
        .route("/jokes.json", get(jokes_json))
        .route("/brands/everscale.svg", get(everscale_logo))
        .route("/brands/tycho.svg", get(tycho_logo))
        .route("/brands/ton.svg", get(ton_logo))
        .route("/brands/smoking-man.png", get(smoking_man_png))
        .route("/brands/portraits/{name}", get(portrait_image))
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route(
            "/api/analytics/event",
            post(analytics_event).layer(DefaultBodyLimit::max(1024)),
        )
        .route("/api/analytics/public", get(public_analytics))
        .route("/api/chains", get(list_chains))
        .route("/api/chains/{chain_id}/clock", get(chain_clock))
        .route("/api/chains/{chain_id}/map", get(chain_map))
        .route("/api/chains/{chain_id}/round-stats", get(chain_round_stats))
        .fallback(not_found)
        .with_state(state)
        .layer(layers)
}

fn stats_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/stats", get(stats_page))
        .route("/stats/", get(stats_page))
        .route("/stats/app.js", get(stats_js))
        .route("/stats/visitors", get(public_visitors))
        .layer(middleware::from_fn_with_state(state, require_stats_auth))
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
