use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use ts_rs::TS;

use crate::mealie::MealieError;

#[derive(Debug)]
pub enum AppError {
    Unauthorized,
    Forbidden,
    MealieUnreachable,
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
            Self::Unauthorized => Wire {
                status: StatusCode::UNAUTHORIZED,
                kind: "unauthorized",
                message: "A valid Mealie API key is required.",
            },
            Self::Forbidden => Wire {
                status: StatusCode::FORBIDDEN,
                kind: "forbidden",
                message: "This link is invalid or has expired.",
            },
            Self::MealieUnreachable => Wire {
                status: StatusCode::BAD_GATEWAY,
                kind: "mealie_unreachable",
                message: "Mealie did not respond.",
            },
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

    pub fn from_identity_lookup(error: &MealieError) -> Self {
        match error {
            MealieError::Unauthorized => Self::Unauthorized,
            MealieError::Unreachable(_)
            | MealieError::Rejected(_)
            | MealieError::NotFound
            | MealieError::Other(_) => {
                tracing::warn!(?error, "mealie call failed");
                Self::MealieUnreachable
            }
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
