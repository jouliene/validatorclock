use crate::config::AppConfig;
use crate::hostname::normalize_host;
use crate::server::responses::{json_error, not_found};
use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

const STATS_AUTH_CHALLENGE: HeaderValue =
    HeaderValue::from_static("Basic realm=\"Validator Clock stats\", charset=\"UTF-8\"");
const FAILED_AUTH_DELAY: Duration = Duration::from_millis(250);

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

pub(super) async fn require_stats_auth(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let stats_auth = &state.config.security.stats_auth;
    if !stats_auth.enabled {
        return next.run(request).await;
    }

    // Without a password the page cannot be protected, so it stays hidden
    // instead of being served to everyone.
    let Some(password) = stats_auth.effective_password() else {
        return not_found().await;
    };

    let offered = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(basic_credentials);

    match offered {
        Some((username, offered_password))
            if secret_eq(&username, &stats_auth.username)
                && secret_eq(&offered_password, &password) =>
        {
            let mut response = next.run(request).await;
            if !response.headers().contains_key(header::CACHE_CONTROL) {
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            }
            response
        }
        offered => {
            if offered.is_some() {
                tokio::time::sleep(FAILED_AUTH_DELAY).await;
            }
            unauthorized_response()
        }
    }
}

fn unauthorized_response() -> Response {
    let mut response = json_error(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "authentication required",
    );
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, STATS_AUTH_CHALLENGE);
    response
}

fn basic_credentials(header_value: &str) -> Option<(String, String)> {
    let encoded = header_value
        .strip_prefix("Basic ")
        .or_else(|| header_value.strip_prefix("basic "))?
        .trim();
    let decoded = BASE64.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_owned(), password.to_owned()))
}

fn secret_eq(left: &str, right: &str) -> bool {
    // Digests are compared instead of the raw secrets so neither the contents
    // nor the length of the expected value leaks through timing.
    let left = Sha256::digest(left.as_bytes());
    let right = Sha256::digest(right.as_bytes());
    left.iter()
        .zip(right.iter())
        .fold(0u8, |differences, (left, right)| {
            differences | (left ^ right)
        })
        == 0
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
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
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
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; worker-src 'self' blob:; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000"),
    );
}

pub(super) fn redirect_location(public_url: &str, target: &str) -> String {
    format!("{}{}", public_url.trim_end_matches('/'), target)
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
