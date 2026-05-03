use crate::config::AppConfig;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Request, State};
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

pub(super) async fn handle_options(request: Request, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    }
}

pub(super) async fn enforce_allowed_host(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if !request_host_allowed(request.headers(), &state.config) {
        return json_error(StatusCode::BAD_REQUEST, "bad_host", "bad host");
    }

    next.run(request).await
}

pub(super) async fn add_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    add_common_headers(response.headers_mut());
    response
}

pub(super) fn redirect_response(state: &AppState, headers: &HeaderMap, uri: &Uri) -> Response {
    if !request_host_allowed(headers, &state.config) {
        return json_error(StatusCode::BAD_REQUEST, "bad_host", "bad host");
    }

    let location = redirect_location(
        &state.config.tls.public_url,
        uri.path_and_query()
            .map(|value| value.as_str())
            .unwrap_or("/"),
    );
    let Ok(location) = HeaderValue::from_str(&location) else {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_redirect_location",
            "invalid redirect location",
        );
    };

    (
        StatusCode::PERMANENT_REDIRECT,
        [(header::LOCATION, location)],
    )
        .into_response()
}

fn add_common_headers(headers: &mut HeaderMap) {
    if !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000"),
    );
}

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

pub(super) fn redirect_location(public_url: &str, target: &str) -> String {
    format!("{}{}", public_url.trim_end_matches('/'), target)
}

pub(crate) fn public_url_host(public_url: &str) -> Option<String> {
    let rest = public_url.strip_prefix("https://")?;
    let host = rest.split('/').next().unwrap_or(rest);
    normalize_host(host)
}

pub(crate) fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }

    if host.starts_with('[')
        && let Some(end) = host.find(']')
    {
        let value = &host[1..end];
        return Some(
            value
                .parse::<IpAddr>()
                .map(|address| address.to_string())
                .unwrap_or_else(|_| value.to_ascii_lowercase()),
        );
    }

    if let Ok(address) = host.parse::<IpAddr>() {
        return Some(address.to_string());
    }

    let host_without_port = host
        .rsplit_once(':')
        .filter(|(name, port)| !name.contains(':') && port.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(name, _)| name)
        .unwrap_or(host);

    Some(host_without_port.trim_end_matches('.').to_ascii_lowercase())
}

pub(super) fn request_host_allowed(headers: &HeaderMap, config: &AppConfig) -> bool {
    let allowed_hosts = config.effective_allowed_hosts();
    if allowed_hosts.is_empty() {
        return true;
    }

    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_host)
    else {
        return false;
    };

    allowed_hosts
        .iter()
        .filter_map(|host| normalize_host(host))
        .any(|allowed| allowed == host)
}

pub(super) fn query_forces_refresh(query: &HashMap<String, String>) -> bool {
    query
        .get("refresh")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}
