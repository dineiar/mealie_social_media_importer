use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("request body too large")]
    PayloadTooLarge,
    #[error("the request took too long")]
    GatewayTimeout,
}

impl AppError {
    fn status_and_kind(&self) -> (StatusCode, &'static str) {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
            Self::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            Self::GatewayTimeout => (StatusCode::GATEWAY_TIMEOUT, "timeout"),
        }
    }

    pub fn from_middleware_status(status: StatusCode) -> Option<Self> {
        match status {
            StatusCode::PAYLOAD_TOO_LARGE => Some(Self::PayloadTooLarge),
            StatusCode::GATEWAY_TIMEOUT => Some(Self::GatewayTimeout),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    kind: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, kind) = self.status_and_kind();
        let error = ErrorDetail {
            kind,
            message: self.to_string(),
        };
        (status, Json(ErrorBody { error })).into_response()
    }
}
