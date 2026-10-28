use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
}

impl AppError {
    fn status_and_kind(&self) -> (StatusCode, &'static str) {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
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
