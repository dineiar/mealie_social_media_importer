mod config;
mod files;
mod health;
mod imports;
mod me;
mod platforms;

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Response};
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

/// Above the 600 s `?wait=true` cap, and equal to nginx's `proxy_read_timeout` on `/api`.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(660);

pub fn router(state: AppState) -> Router {
    with_middleware(
        Router::new()
            .route("/api/health", get(health::health))
            .route("/api/config", get(config::config))
            .route("/api/me", get(me::me))
            .route("/api/platforms", get(platforms::platforms))
            .route("/api/imports", get(imports::list))
            .route("/api/imports/{id}", get(imports::get))
            .route("/api/files/{id}/{name}", get(files::download))
            .fallback(|| async { AppError::NotFound })
            .method_not_allowed_fallback(|| async { AppError::MethodNotAllowed }),
    )
    .with_state(state)
}

fn with_middleware<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(
            ServiceBuilder::new()
                .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::GATEWAY_TIMEOUT,
                    RESPONSE_TIMEOUT,
                )),
        )
        .layer(map_response(json_middleware_errors))
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(TraceLayer::new_for_http().make_span_with(request_span))
                .layer(PropagateRequestIdLayer::x_request_id()),
        )
}

async fn json_middleware_errors(response: Response) -> Response {
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"application/json"));
    match AppError::from_middleware_status(response.status()) {
        Some(error) if !is_json => error.into_response(),
        _ => response,
    }
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

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_response_that_never_starts_is_a_json_gateway_timeout() {
        let app = with_middleware(
            Router::<()>::new().route("/slow", get(std::future::pending::<&'static str>)),
        );

        let response = app
            .oneshot(Request::get("/slow").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body,
            serde_json::json!({ "error": { "kind": "timeout", "message": "The request took too long." } })
        );
    }
}
