mod config;
mod health;

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceBuilder;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{
    MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::AppState;
use crate::error::AppError;

const BODY_LIMIT_BYTES: usize = 64 * 1024;

/// `TimeoutLayer` bounds only the time to the response head, so a streamed body is never cut.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(660);

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health::health))
        .route("/api/config", get(config::config))
        .fallback(|| async { AppError::NotFound })
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(TraceLayer::new_for_http().make_span_with(request_span))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::GATEWAY_TIMEOUT,
                    RESPONSE_TIMEOUT,
                )),
        )
        .with_state(state)
}

fn request_span(request: &Request<Body>) -> tracing::Span {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or_default();
    tracing::info_span!(
        "request",
        method = %request.method(),
        path = request.uri().path(),
        request_id,
    )
}
