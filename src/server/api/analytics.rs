use crate::state::AppState;
use crate::state::analytics::{AnalyticsEventKind, TrafficAttribution};
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
    #[serde(default)]
    referrer_origin: Option<String>,
    #[serde(default)]
    utm_source: Option<String>,
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
    let Some(payload) = parse_event_body(body) else {
        return StatusCode::NO_CONTENT;
    };

    let event = if payload.event == "page_open" {
        AnalyticsEventKind::PageOpen
    } else {
        AnalyticsEventKind::Heartbeat
    };
    let attribution = payload
        .path
        .as_deref()
        .filter(|path| is_public_page(&state, path))
        .map(|path| TrafficAttribution {
            landing_page: path.to_owned(),
            source: traffic_source(
                payload.referrer_origin.as_deref(),
                payload.utm_source.as_deref(),
            ),
        });
    state
        .record_analytics_event(event, peer_addr, &headers, attribution)
        .await;
    StatusCode::NO_CONTENT
}

pub(in crate::server) async fn public_analytics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(state.public_analytics().await)
}

pub(in crate::server) async fn public_visitors(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(state.public_visitors().await)
}

fn parse_event_body(body: Bytes) -> Option<AnalyticsEventPayload> {
    let payload = serde_json::from_slice::<AnalyticsEventPayload>(&body).ok()?;
    let _ = (&payload.path, payload.visible, payload.ts);
    match payload.event.as_str() {
        "page_open" | "heartbeat" => Some(payload),
        _ => None,
    }
}

pub(in crate::server) async fn traffic_report(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(state.traffic_report().await)
}

fn is_public_page(_state: &AppState, path: &str) -> bool {
    path == "/"
}

fn traffic_source(origin: Option<&str>, campaign: Option<&str>) -> &'static str {
    let campaign = campaign.unwrap_or("").to_ascii_lowercase();
    let campaign_source = match campaign.as_str() {
        "chatgpt" | "chatgpt.com" => Some("ChatGPT"),
        "perplexity" | "perplexity.ai" => Some("Perplexity"),
        "google" => Some("Google"),
        "yandex" => Some("Yandex"),
        "bing" => Some("Bing"),
        "claude" | "claude.ai" => Some("Claude"),
        _ => None,
    };
    if let Some(source) = campaign_source {
        return source;
    }
    let host = origin
        .and_then(|origin| reqwest::Url::parse(origin).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    let matches_domain = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if matches_domain("chatgpt.com") || matches_domain("chat.openai.com") {
        "ChatGPT"
    } else if matches_domain("perplexity.ai") || matches_domain("perplexity.com") {
        "Perplexity"
    } else if matches_domain("claude.ai") {
        "Claude"
    } else if [
        "google.com",
        "google.ru",
        "google.co.uk",
        "google.com.au",
        "google.de",
        "google.fr",
    ]
    .iter()
    .any(|domain| matches_domain(domain))
    {
        "Google"
    } else if ["yandex.ru", "yandex.com", "yandex.kz", "ya.ru", "yandex.by"]
        .iter()
        .any(|domain| matches_domain(domain))
    {
        "Yandex"
    } else if matches_domain("bing.com") {
        "Bing"
    } else if host.is_empty() && campaign.is_empty() {
        "Direct or unknown"
    } else {
        "Other referral or campaign"
    }
}

#[cfg(test)]
mod tests {
    use super::traffic_source;

    #[test]
    fn groups_sources_without_storing_arbitrary_domains_or_campaigns() {
        assert_eq!(
            traffic_source(Some("https://www.google.com"), None),
            "Google"
        );
        assert_eq!(traffic_source(Some("https://yandex.ru"), None), "Yandex");
        assert_eq!(traffic_source(None, Some("chatgpt.com")), "ChatGPT");
        assert_eq!(
            traffic_source(Some("https://google.com.attacker.example"), None),
            "Other referral or campaign"
        );
        assert_eq!(
            traffic_source(None, Some("private-campaign")),
            "Other referral or campaign"
        );
        assert_eq!(traffic_source(None, None), "Direct or unknown");
    }
}
