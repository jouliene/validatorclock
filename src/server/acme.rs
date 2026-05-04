use super::security::redirect_response;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::header::{self, HeaderValue};
use axum::http::{HeaderMap, Uri};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

pub(super) async fn acme_challenge(
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

pub(super) async fn redirect_to_https(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    redirect_response(&state, &headers, &uri)
}
