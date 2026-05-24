use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

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

pub(super) async fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "not found")
}
