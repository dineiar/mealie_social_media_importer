use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use mealie_importer::config::Config;
use mealie_importer::mealie::MealieClient;
use mealie_importer::{AppState, router};
use serde_json::{Value, json};
use tower::ServiceExt;

fn app(pairs: &[(&str, &str)]) -> Router {
    let vars: HashMap<String, String> = pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    let config = Config::from_vars(&vars).expect("configuration is valid");
    let mealie = MealieClient::from_config(&config).expect("client builds");
    router(AppState::new(config, Arc::new(mealie)))
}

fn default_app() -> Router {
    app(&[("MEALIE_URL", "http://mealie:9000")])
}

async fn send(app: Router, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let response = app.oneshot(request).await.expect("router answers");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, body)
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn health_reports_ok() {
    let (status, _, body) = send(default_app(), get("/api/health")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn config_exposes_public_url_retention_and_version() {
    let app = app(&[
        ("MEALIE_URL", "http://mealie:9000"),
        ("MEALIE_PUBLIC_URL", "https://recipes.example.com"),
        ("MEDIA_RETENTION_MINUTES", "90"),
    ]);

    let (status, _, body) = send(app, get("/api/config")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "mealie_public_url": "https://recipes.example.com/",
            "retention_minutes": 90,
            "version": env!("CARGO_PKG_VERSION"),
        })
    );
}

#[tokio::test]
async fn unknown_path_is_a_json_not_found_error() {
    let (status, _, body) = send(default_app(), get("/api/nothing-here")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({ "error": { "kind": "not_found", "message": "Not found." } })
    );
}

#[tokio::test]
async fn responses_carry_a_request_id() {
    let (_, headers, _) = send(default_app(), get("/api/health")).await;
    assert!(headers.contains_key("x-request-id"));

    let request = Request::get("/api/health")
        .header("x-request-id", "abc-123")
        .body(Body::empty())
        .unwrap();
    let (_, headers, _) = send(default_app(), request).await;
    assert_eq!(headers["x-request-id"], "abc-123");
}

#[tokio::test]
async fn oversized_body_is_a_json_payload_too_large_error() {
    let body = vec![b'a'; 70_000];
    let request = Request::post("/api/health")
        .header("content-length", body.len())
        .body(Body::from(body))
        .unwrap();

    let (status, headers, body) = send(default_app(), request).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(
        body,
        json!({ "error": { "kind": "payload_too_large", "message": "Request body too large." } })
    );
}

#[tokio::test]
async fn wrong_method_is_a_json_method_not_allowed_error() {
    let request = Request::post("/api/health").body(Body::empty()).unwrap();

    let (status, headers, body) = send(default_app(), request).await;

    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(
        body,
        json!({ "error": { "kind": "method_not_allowed", "message": "Method not allowed." } })
    );
}
