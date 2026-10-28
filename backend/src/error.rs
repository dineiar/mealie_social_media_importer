use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug)]
pub enum AppError {
    NotFound,
    MethodNotAllowed,
    PayloadTooLarge,
    GatewayTimeout,
}

struct Wire {
    status: StatusCode,
    kind: &'static str,
    message: &'static str,
}

impl AppError {
    const fn wire(&self) -> Wire {
        match self {
            Self::NotFound => Wire {
                status: StatusCode::NOT_FOUND,
                kind: "not_found",
                message: "Not found.",
            },
            Self::MethodNotAllowed => Wire {
                status: StatusCode::METHOD_NOT_ALLOWED,
                kind: "method_not_allowed",
                message: "Method not allowed.",
            },
            Self::PayloadTooLarge => Wire {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                kind: "payload_too_large",
                message: "Request body too large.",
            },
            Self::GatewayTimeout => Wire {
                status: StatusCode::GATEWAY_TIMEOUT,
                kind: "timeout",
                message: "The request took too long.",
            },
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

#[derive(Serialize, TS)]
#[ts(export)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize, TS)]
#[ts(export)]
struct ErrorDetail {
    kind: &'static str,
    message: &'static str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let Wire {
            status,
            kind,
            message,
        } = self.wire();
        let error = ErrorDetail { kind, message };
        (status, Json(ErrorBody { error })).into_response()
    }
}
