use std::fs;
use std::path::Path;

use axum::Router;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::support::{bearer, config_with, get_json, router_for, serve, unauthorized, users_ok};

const LOGGED_IN: &str =
    "# Netscape HTTP Cookie File\n.instagram.com\tTRUE\t/\tTRUE\t0\tsessionid\ts1\n";

fn app(mealie_url: &str, data_dir: &Path, cookies_file: &Path) -> Router {
    router_for(config_with(&[
        ("MEALIE_URL", mealie_url),
        ("DATA_DIR", data_dir.to_str().unwrap()),
        ("INSTAGRAM_COOKIES_FILE", cookies_file.to_str().unwrap()),
    ]))
}

#[tokio::test]
async fn the_list_needs_a_key() {
    let mealie = serve(users_ok()).await;
    let dir = TempDir::new().unwrap();
    let app = app(&mealie.uri(), dir.path(), &dir.path().join("instagram.txt"));

    let (status, body) = get_json(&app, "/api/platforms", None).await;

    assert_eq!((status, body), (StatusCode::UNAUTHORIZED, unauthorized()));
}

#[tokio::test]
async fn instagram_turns_ready_when_the_cookies_file_appears_without_a_restart() {
    let mealie = serve(users_ok()).await;
    let dir = TempDir::new().unwrap();
    let cookies = dir.path().join("config/instagram.txt");
    let app = app(&mealie.uri(), &dir.path().join("data"), &cookies);

    let (status, body) = get_json(&app, "/api/platforms", Some(&bearer())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([{
            "id": "instagram",
            "name": "Instagram",
            "status": "not_configured",
            "hint": "cookies_missing",
        }])
    );

    fs::create_dir_all(cookies.parent().unwrap()).unwrap();
    fs::write(&cookies, LOGGED_IN).unwrap();

    let (status, body) = get_json(&app, "/api/platforms", Some(&bearer())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([{
            "id": "instagram",
            "name": "Instagram",
            "status": "ready",
            "hint": null,
        }])
    );
}
