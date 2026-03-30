use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// 統一エラーボディ。message は省略可能。
#[derive(Serialize)]
pub struct ErrorBody {
    pub error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub fn error_resp(status: StatusCode, code: &'static str, message: Option<&str>) -> Response {
    (status, Json(ErrorBody {
        error: code,
        message: message.map(|s| s.to_string()),
    }))
    .into_response()
}

pub fn unauthorized() -> Response {
    error_resp(
        StatusCode::UNAUTHORIZED,
        "Unauthorized",
        Some("invalid or missing Authorization header"),
    )
}

pub fn not_found() -> Response {
    error_resp(StatusCode::NOT_FOUND, "NotFound", Some("object not found"))
}

pub fn invalid_key() -> Response {
    error_resp(
        StatusCode::BAD_REQUEST,
        "InvalidKey",
        Some("key must not be empty, start with /, or contain .."),
    )
}

pub fn server_error<E: std::fmt::Display>(e: E) -> Response {
    error_resp(
        StatusCode::INTERNAL_SERVER_ERROR,
        "InternalError",
        Some(&e.to_string()),
    )
}
