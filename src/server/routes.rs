use super::acme::{acme_challenge, redirect_to_https};
use super::api::{chain_clock, health, list_chains, not_found, status};
use super::assets::{app_js, everscale_logo, index, jokes_json, styles, tycho_logo};
use super::security::{add_security_headers, enforce_allowed_host, handle_options};
use crate::state::AppState;
use axum::Router;
use axum::middleware;
use axum::routing::get;
use std::sync::Arc;
use tower::ServiceBuilder;

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
        .route("/jokes.json", get(jokes_json))
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
