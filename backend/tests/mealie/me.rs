use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{
    KEY, authorization, bearer, calls_to, capture_logs, config, error_body, get_json, groups_ok,
    refused_url, router_for, unauthorized, user_json, users_ok,
};

fn app(mealie_url: &str) -> Router {
    router_for(config(mealie_url))
}

async fn get_me(app: &Router, header: Option<&str>) -> (StatusCode, Value) {
    get_json(app, "/api/me", header).await
}

async fn mealie_answering(users: ResponseTemplate, groups: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    for (route, template) in [("/api/users/self", users), ("/api/groups/self", groups)] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(template)
            .mount(&server)
            .await;
    }
    server
}

fn mealie_unreachable() -> Value {
    error_body("mealie_unreachable", "Mealie did not respond.")
}

#[tokio::test]
async fn a_valid_key_gets_the_user_and_the_ai_settings() {
    let server = mealie_answering(users_ok(), groups_ok()).await;

    let (status, body) = get_me(&app(&server.uri()), Some(&bearer())).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "id": "eaab0b0e-0df9-402c-8f5c-90cced537b01",
            "username": "admin",
            "full_name": "Change Me",
            "group_slug": "home",
            "ai": { "enabled": true, "image_enabled": false },
        })
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_eq!(authorization(&request), bearer());
    }
}

#[tokio::test]
async fn a_user_without_a_name_has_null_name_fields() {
    let mut user = user_json();
    user["username"] = Value::Null;
    user["fullName"] = Value::Null;
    let server =
        mealie_answering(ResponseTemplate::new(200).set_body_json(user), groups_ok()).await;

    let (status, body) = get_me(&app(&server.uri()), Some(&bearer())).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (&body["username"], &body["full_name"]),
        (&Value::Null, &Value::Null)
    );
}

#[tokio::test]
async fn the_authorization_scheme_is_case_insensitive() {
    let server = mealie_answering(users_ok(), groups_ok()).await;

    let (status, _) = get_me(&app(&server.uri()), Some(&format!("bearer {KEY}"))).await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_request_without_a_usable_key_is_unauthorized_and_never_reaches_mealie() {
    let server = mealie_answering(users_ok(), groups_ok()).await;
    let app = app(&server.uri());

    for header in [
        None,
        Some("Basic dXNlcjpwYXNzd29yZA=="),
        Some("Bearer"),
        Some("Bearer "),
        Some("Bearer    "),
        Some(KEY),
    ] {
        let (status, body) = get_me(&app, header).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED, "{header:?}");
        assert_eq!(body, unauthorized(), "{header:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_key_mealie_rejects_is_unauthorized() {
    for rejection in [401, 403] {
        let server = mealie_answering(ResponseTemplate::new(rejection), groups_ok()).await;

        let (status, body) = get_me(&app(&server.uri()), Some(&bearer())).await;

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Mealie answered {rejection}"
        );
        assert_eq!(body, unauthorized());
    }
}

#[tokio::test]
async fn a_mealie_that_cannot_be_reached_is_a_bad_gateway() {
    let (status, body) = get_me(&app(&refused_url()), Some(&bearer())).await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body, mealie_unreachable());
}

#[tokio::test]
async fn any_other_users_self_answer_is_a_bad_gateway() {
    let answers = [
        ResponseTemplate::new(400),
        ResponseTemplate::new(404),
        ResponseTemplate::new(500),
        ResponseTemplate::new(302).insert_header("location", "https://elsewhere.test/"),
        ResponseTemplate::new(200).set_body_string("<html>not Mealie</html>"),
    ];
    for answer in answers {
        let server = mealie_answering(answer, groups_ok()).await;

        let (status, body) = get_me(&app(&server.uri()), Some(&bearer())).await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body, mealie_unreachable());
    }
}

#[tokio::test]
async fn a_groups_self_failure_is_a_bad_gateway() {
    let server = mealie_answering(users_ok(), ResponseTemplate::new(500)).await;

    let (status, body) = get_me(&app(&server.uri()), Some(&bearer())).await;

    assert_eq!(
        (status, body),
        (StatusCode::BAD_GATEWAY, mealie_unreachable())
    );
}

#[tokio::test]
async fn a_second_me_within_the_ttl_makes_no_users_self_call() {
    let server = mealie_answering(users_ok(), groups_ok()).await;
    let app = app(&server.uri());

    let (first, _) = get_me(&app, Some(&bearer())).await;
    let (second, _) = get_me(&app, Some(&bearer())).await;

    assert_eq!((first, second), (StatusCode::OK, StatusCode::OK));
    assert_eq!(calls_to(&server, "/api/users/self").await, 1);
}

#[tokio::test]
async fn an_unauthorized_groups_self_evicts_the_cached_user() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/users/self"))
        .respond_with(users_ok())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/groups/self"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/groups/self"))
        .respond_with(groups_ok())
        .mount(&server)
        .await;
    let app = app(&server.uri());

    let (first, _) = get_me(&app, Some(&bearer())).await;
    let (second, _) = get_me(&app, Some(&bearer())).await;

    assert_eq!((first, second), (StatusCode::UNAUTHORIZED, StatusCode::OK));
    assert_eq!(calls_to(&server, "/api/users/self").await, 2);
}

#[tokio::test]
async fn the_request_trace_and_the_error_logs_never_hold_the_key() {
    let logs = capture_logs();
    let broken = mealie_answering(users_ok(), ResponseTemplate::new(500)).await;
    let rejecting = mealie_answering(ResponseTemplate::new(401), groups_ok()).await;
    let healthy = mealie_answering(users_ok(), groups_ok()).await;

    for url in [healthy.uri(), broken.uri(), rejecting.uri(), refused_url()] {
        get_me(&app(&url), Some(&bearer())).await;
    }
    get_me(&app(&healthy.uri()), None).await;

    let logged = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(
        logged.contains("finished processing request"),
        "the trace layer logs each response"
    );
    assert!(
        logged.contains("mealie call failed"),
        "a failed Mealie call is logged"
    );
    assert!(!logged.contains(KEY), "a log line holds the key:\n{logged}");
}
