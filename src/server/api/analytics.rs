use crate::state::AppState;
use crate::state::analytics::AnalyticsEventKind;
use axum::Json;
use axum::body::{Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

const MAX_ANALYTICS_BODY_BYTES: usize = 1024;

#[derive(Debug, Deserialize)]
struct AnalyticsEventPayload {
    event: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    ts: Option<u64>,
}

pub(in crate::server) async fn analytics_event(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> StatusCode {
    let peer_addr = request.extensions().get::<SocketAddr>().copied();
    let headers = request.headers().clone();
    let Ok(body) = to_bytes(request.into_body(), MAX_ANALYTICS_BODY_BYTES).await else {
        return StatusCode::NO_CONTENT;
    };
    let Some(event) = parse_event_body(body) else {
        return StatusCode::NO_CONTENT;
    };

    state
        .record_analytics_event(event, peer_addr, &headers)
        .await;
    StatusCode::NO_CONTENT
}

pub(in crate::server) async fn public_analytics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(state.public_analytics().await)
}

fn parse_event_body(body: Bytes) -> Option<AnalyticsEventKind> {
    let payload = serde_json::from_slice::<AnalyticsEventPayload>(&body).ok()?;
    let _ = (&payload.path, payload.visible, payload.ts);
    match payload.event.as_str() {
        "page_open" => Some(AnalyticsEventKind::PageOpen),
        "heartbeat" => Some(AnalyticsEventKind::Heartbeat),
        _ => None,
    }
}
