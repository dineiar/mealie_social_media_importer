use std::fs::{self, File};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use mealie_importer::mealie::{
    AiRecipeRequest, AssetUpload, MealieApi, MealieError, MealieGroup, MealieUser, Recipe,
    RecipePatch, RecipeSlug,
};
use mealie_importer::store::ImportErrorKind;
use mealie_importer::{AppState, router};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::support::{Harness, RETENTION, add_file, alice, bob, capture_logs, exp_and_sig, json};

const ALICE_KEY: &str = "alice-key";
const BOB_KEY: &str = "bob-key";

struct FakeMealie;

#[async_trait]
impl MealieApi for FakeMealie {
    async fn users_self(&self, key: &SecretString) -> Result<MealieUser, MealieError> {
        let id = match key.expose_secret() {
            ALICE_KEY => "alice-id",
            BOB_KEY => "bob-id",
            _ => return Err(MealieError::Unauthorized),
        };
        Ok(MealieUser {
            id: id.to_owned(),
            username: None,
            full_name: None,
            group_slug: "home".to_owned(),
        })
    }

    async fn groups_self(&self, _: &SecretString) -> Result<MealieGroup, MealieError> {
        Err(MealieError::NotFound)
    }

    async fn create_from_ai(
        &self,
        _: &SecretString,
        _: &AiRecipeRequest,
    ) -> Result<RecipeSlug, MealieError> {
        Err(MealieError::NotFound)
    }

    async fn get_recipe(&self, _: &SecretString, _: &RecipeSlug) -> Result<Recipe, MealieError> {
        Err(MealieError::NotFound)
    }

    async fn patch_recipe(
        &self,
        _: &SecretString,
        _: &RecipeSlug,
        _: &RecipePatch,
    ) -> Result<Recipe, MealieError> {
        Err(MealieError::NotFound)
    }

    async fn delete_recipe(&self, _: &SecretString, _: &RecipeSlug) -> Result<(), MealieError> {
        Err(MealieError::NotFound)
    }

    async fn upload_image(
        &self,
        _: &SecretString,
        _: &RecipeSlug,
        _: &Path,
    ) -> Result<(), MealieError> {
        Err(MealieError::NotFound)
    }

    async fn upload_asset(
        &self,
        _: &SecretString,
        _: &RecipeSlug,
        _: &AssetUpload,
    ) -> Result<(), MealieError> {
        Err(MealieError::NotFound)
    }
}

fn app(harness: &Harness) -> Router {
    let state = AppState::new(harness.config(), Arc::new(FakeMealie))
        .with_imports(Arc::clone(&harness.store));
    router(state)
}

async fn send(app: &Router, uri: &str, key: Option<&str>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut request = Request::get(uri);
    if let Some(key) = key {
        request = request.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, body.to_vec())
}

async fn get_json(app: &Router, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
    let (status, _, body) = send(app, uri, key).await;
    (status, serde_json::from_slice(&body).expect("a JSON body"))
}

fn error(kind: &str, message: &str) -> Value {
    json!({ "error": { "kind": kind, "message": message } })
}

fn forbidden() -> Value {
    error("forbidden", "This link is invalid or has expired.")
}

fn not_found() -> Value {
    error("not_found", "Not found.")
}

fn link(harness: &Harness, file: usize) -> String {
    let import = harness.only(&alice());
    import["files"][file]["url"].as_str().unwrap().to_owned()
}

fn with_query(link: &str, query: &str) -> String {
    let (path, _) = link.split_once('?').unwrap();
    format!("{path}?{query}")
}

#[tokio::test]
async fn listing_needs_a_key_and_returns_only_the_callers_imports_newest_first() {
    let harness = Harness::new();
    let older = harness.create(&alice(), "https://www.instagram.com/p/older/");
    let bobs = harness.create(&bob(), "https://www.instagram.com/p/bobs/");
    let newer = harness.create(&alice(), "https://www.instagram.com/p/newer/");
    let app = app(&harness);

    let (status, body) = get_json(&app, "/api/imports", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        body,
        error("unauthorized", "A valid Mealie API key is required.")
    );

    let (status, body) = get_json(&app, "/api/imports", Some(ALICE_KEY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        Value::Array(harness.store.list(&alice()).iter().map(json).collect())
    );
    let ids: Vec<&Value> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|import| &import["id"])
        .collect();
    assert_eq!(
        ids,
        [
            &json!(newer.id().to_string()),
            &json!(older.id().to_string())
        ]
    );

    let (_, body) = get_json(&app, "/api/imports", Some(BOB_KEY)).await;
    assert_eq!(body[0]["id"], bobs.id().to_string());
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn one_import_is_found_only_by_its_owner() {
    let harness = Harness::new();
    let writer = harness.create(&alice(), "https://www.instagram.com/p/mine/");
    let app = app(&harness);
    let uri = format!("/api/imports/{}", writer.id());

    let (status, body) = get_json(&app, &uri, Some(ALICE_KEY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, harness.only(&alice()));

    for (uri, key) in [
        (uri.as_str(), BOB_KEY),
        ("/api/imports/not-an-id", ALICE_KEY),
        (
            "/api/imports/0b6c2f4e-8a1d-4c2e-9f4b-2f1d7a9e3c10",
            ALICE_KEY,
        ),
        (&uri.to_uppercase(), ALICE_KEY),
    ] {
        assert_eq!(
            get_json(&app, uri, Some(key)).await,
            (StatusCode::NOT_FOUND, not_found()),
            "{uri} as {key}"
        );
    }

    let (status, _) = get_json(&app, &uri, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_signed_link_downloads_the_file_as_an_attachment_without_a_key() {
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");
    add_file(&mut writer, "video-1.mp4", b"not really an mp4");
    add_file(&mut writer, "caption.txt", "Lemon orzo 🍋".as_bytes());
    let app = app(&harness);

    let (status, headers, body) = send(&app, &link(&harness, 0), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "video/mp4");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"video-1.mp4\""
    );
    assert_eq!(headers[header::CONTENT_LENGTH], "17");
    assert_eq!(body, b"not really an mp4");

    let (status, headers, body) = send(&app, &link(&harness, 1), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/plain");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"caption.txt\""
    );
    assert_eq!(body, "Lemon orzo 🍋".as_bytes());
}

#[tokio::test]
async fn a_tampered_or_incomplete_link_is_forbidden() {
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");
    add_file(&mut writer, "video-1.mp4", b"mp4");
    add_file(&mut writer, "caption.txt", b"caption");
    let app = app(&harness);
    let video = link(&harness, 0);
    let (exp, sig) = exp_and_sig(&video);
    let caption_path = link(&harness, 1).split_once('?').unwrap().0.to_owned();
    let flipped: String = sig
        .chars()
        .enumerate()
        .map(|(i, c)| match (i, c) {
            (0, '0') => '1',
            (0, _) => '0',
            (_, c) => c,
        })
        .collect();
    let later = exp.parse::<i64>().unwrap() + 3600;
    let mut other = harness.create(&alice(), "https://www.instagram.com/reel/other/");
    add_file(&mut other, "video-1.mp4", b"mp4");
    let other = other.id();

    for uri in [
        with_query(&video, &format!("exp={exp}&sig={flipped}")),
        with_query(&video, &format!("exp={later}&sig={sig}")),
        with_query(&video, &format!("exp={exp}&sig={}", sig.to_uppercase())),
        with_query(&video, &format!("exp={exp}&sig={}", &sig[..62])),
        with_query(&video, &format!("exp={exp}")),
        with_query(&video, &format!("sig={sig}")),
        with_query(&video, &format!("exp=soon&sig={sig}")),
        with_query(&video, &format!("exp={exp}&sig={sig}&sig={sig}")),
        format!("{caption_path}?exp={exp}&sig={sig}"),
        format!("/api/files/{other}/video-1.mp4?exp={exp}&sig={sig}"),
    ] {
        let (status, body) = get_json(&app, &uri, None).await;
        assert_eq!(
            (status, body),
            (StatusCode::FORBIDDEN, forbidden()),
            "{uri}"
        );
    }

    let (status, _, _) = send(&app, &video, None).await;
    assert_eq!(status, StatusCode::OK, "the untouched link still works");
}

#[tokio::test]
async fn a_link_stops_working_when_it_expires() {
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");
    add_file(&mut writer, "video-1.mp4", b"mp4");
    let app = app(&harness);
    let running_link = link(&harness, 0);

    harness.clock.advance(RETENTION - Duration::from_secs(1));
    assert_eq!(send(&app, &running_link, None).await.0, StatusCode::OK);
    harness.clock.advance(Duration::from_secs(1));
    assert_eq!(
        get_json(&app, &running_link, None).await,
        (StatusCode::FORBIDDEN, forbidden()),
        "a link from a poll of a running import lasts one retention period"
    );
    assert_eq!(
        send(&app, &link(&harness, 0), None).await.0,
        StatusCode::OK,
        "the next poll hands out a working link"
    );

    writer.fail(ImportErrorKind::MealieUnreachable);
    let finished_link = link(&harness, 0);
    harness.clock.advance(RETENTION - Duration::from_secs(1));
    assert_eq!(send(&app, &link(&harness, 0), None).await.0, StatusCode::OK);
    harness.clock.advance(Duration::from_secs(1));
    assert!(
        harness.list(&alice()).is_empty(),
        "expired imports aren't listed"
    );
    assert_eq!(
        get_json(&app, &finished_link, None).await,
        (StatusCode::FORBIDDEN, forbidden())
    );
}

#[tokio::test]
async fn a_traversal_or_an_unknown_file_is_not_found() {
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");
    add_file(&mut writer, "video-1.mp4", b"mp4");
    add_file(&mut writer, "cover.jpg", b"jpeg");
    let id = writer.id();
    let app = app(&harness);
    let video = link(&harness, 0);
    let (exp, sig) = exp_and_sig(&video);
    let query = format!("exp={exp}&sig={sig}");

    let mut uris: Vec<String> = [
        "..%2Fmanifest.json",
        "..%2F..%2F..%2Fetc%2Fpasswd",
        "%2E%2E",
        "manifest.json",
        "manifest.json.tmp",
        "..%5Cvideo-1.mp4",
        "sub%2Fvideo-1.mp4",
        "video-1.mp4%00.txt",
        "%FF.mp4",
    ]
    .iter()
    .map(|name| format!("/api/files/{id}/{name}?{query}"))
    .collect();
    uris.push(format!("/api/files/{id}/../manifest.json?{query}"));
    uris.push(format!("/api/files/{id}/x/../video-1.mp4?{query}"));
    uris.push(format!("/api/files/not-an-id/video-1.mp4?{query}"));
    for uri in &uris {
        assert_eq!(
            get_json(&app, uri, None).await,
            (StatusCode::NOT_FOUND, not_found()),
            "{uri}"
        );
    }

    fs::remove_file(writer.dir().join("cover.jpg")).unwrap();
    let cover = link(&harness, 1);
    assert_eq!(
        get_json(&app, &cover, None).await,
        (StatusCode::NOT_FOUND, not_found()),
        "a listed file that is gone from disk"
    );
}

#[tokio::test]
async fn a_restart_invalidates_old_links_and_the_next_poll_has_working_ones() {
    let mut harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/p/C9xYz12AbCd/");
    add_file(&mut writer, "cover.jpg", b"jpeg");
    writer.fail(ImportErrorKind::NoRecipe);
    let old_link = link(&harness, 0);

    harness.restart();
    let app = app(&harness);
    assert_eq!(
        get_json(&app, &old_link, None).await,
        (StatusCode::FORBIDDEN, forbidden())
    );
    let new_link = link(&harness, 0);
    assert_ne!(new_link, old_link);
    assert_eq!(send(&app, &new_link, None).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_large_sparse_file_streams_in_small_frames() {
    const SIZE: u64 = 64 * 1024 * 1024;
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");
    File::create(writer.dir().join("video-1.mp4"))
        .unwrap()
        .set_len(SIZE)
        .unwrap();
    writer.add_file("video-1.mp4".parse().unwrap()).unwrap();
    let app = app(&harness);

    let request = Request::get(link(&harness, 0)).body(Body::empty()).unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_LENGTH], SIZE.to_string());
    let mut body = response.into_body();
    let mut largest = 0;
    let mut total = 0;
    while let Some(frame) = body.frame().await {
        let data = frame.unwrap().into_data().unwrap();
        largest = largest.max(data.len());
        total += data.len() as u64;
    }
    assert_eq!(total, SIZE);
    assert!(
        largest <= 64 * 1024,
        "a frame held {largest} bytes, so the file was read whole"
    );
}

#[tokio::test]
async fn the_request_log_never_records_a_links_query() {
    let logs = capture_logs();
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");
    add_file(&mut writer, "video-1.mp4", b"mp4");
    let app = app(&harness);
    let link = link(&harness, 0);
    let (exp, sig) = exp_and_sig(&link);
    let tampered = with_query(&link, &format!("exp={exp}&sig={}", "0".repeat(64)));

    assert_eq!(send(&app, &link, None).await.0, StatusCode::OK);
    assert_eq!(send(&app, &tampered, None).await.0, StatusCode::FORBIDDEN);

    let logged = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    let path = link.split_once('?').unwrap().0;
    assert!(
        logged.contains(&format!("path=\"{path}\"")),
        "the request span logs the path:\n{logged}"
    );
    for secret in [sig.as_str(), &"0".repeat(64), &format!("exp={exp}"), "sig="] {
        assert!(
            !logged.contains(secret),
            "a log line holds {secret}:\n{logged}"
        );
    }
}
