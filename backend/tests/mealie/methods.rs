use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use mealie_importer::config::Config;
use mealie_importer::mealie::{
    AiRecipeRequest, AssetIcon, AssetUpload, Cause, MealieApi, MealieClient, MealieError,
    RecipePatch, RecipeSettings,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{
    Call, Files, KEY, PHOTO_LEN, VIDEO_LEN, ai_request, authorization, bearer, binary,
    capture_logs, client, client_for, group_json, key, kind, multipart, refused_client, serve,
    slug, the_request, user_json,
};

fn recipe_json() -> Value {
    json!({
        "slug": "one-pan-lemon-orzo",
        "name": "One-pan lemon orzo",
        "orgURL": null,
        "recipeIngredient": [
            { "quantity": null, "note": "2 tbsp butter", "display": "2 tbsp butter", "originalText": null },
            { "note": "1 cup orzo", "display": "1 cup orzo" },
        ],
        "recipeInstructions": [
            { "id": "fad0c988", "title": null, "summary": null, "text": "Melt the butter." },
        ],
        "settings": {
            "public": false, "showNutrition": false, "showAssets": false,
            "landscapeView": false, "disableComments": false, "locked": false,
        },
        "assets": [],
    })
}

#[tokio::test]
async fn users_self_returns_the_fields_the_login_needs() {
    let server = serve(ResponseTemplate::new(200).set_body_json(user_json())).await;

    let user = client(&server).users_self(&key()).await.unwrap();

    assert_eq!(user.id, "eaab0b0e-0df9-402c-8f5c-90cced537b01");
    assert_eq!(user.username.as_deref(), Some("admin"));
    assert_eq!(user.full_name.as_deref(), Some("Change Me"));
    assert_eq!(user.group_slug, "home");
    the_request(&server, "GET", "/api/users/self").await;
}

#[tokio::test]
async fn users_self_accepts_a_user_without_a_name() {
    let mut body = user_json();
    body["fullName"] = Value::Null;
    body["username"] = Value::Null;
    let server = serve(ResponseTemplate::new(200).set_body_json(body)).await;

    let user = client(&server).users_self(&key()).await.unwrap();

    assert!(user.username.is_none() && user.full_name.is_none());
}

#[tokio::test]
async fn groups_self_reads_the_ai_provider_settings() {
    let server = serve(ResponseTemplate::new(200).set_body_json(group_json(true, false))).await;

    let group = client(&server).groups_self(&key()).await.unwrap();

    assert!(group.ai_provider_settings.ai_enabled);
    assert!(!group.ai_provider_settings.image_provider_enabled);
    the_request(&server, "GET", "/api/groups/self").await;
}

#[tokio::test]
async fn groups_self_without_ai_settings_has_both_flags_off() {
    let body = json!({ "slug": "home", "aiProviderSettings": null });
    let server = serve(ResponseTemplate::new(200).set_body_json(body)).await;

    let group = client(&server).groups_self(&key()).await.unwrap();

    assert!(!group.ai_provider_settings.ai_enabled);
    assert!(!group.ai_provider_settings.image_provider_enabled);
}

#[tokio::test]
async fn create_from_ai_sends_exactly_the_documented_fields() {
    let server = serve(ResponseTemplate::new(201).set_body_json("one-pan-lemon-orzo")).await;
    let files = Files::new();
    let request = AiRecipeRequest {
        content: "Source: Instagram post by @cookwithmara\n\nCaption:\nOne-pan lemon orzo 🍋"
            .to_owned(),
        images: vec![files.photo.clone(), files.photo.clone()],
        translate_language: Some("pt-BR".to_owned()),
        create_new_organizers: true,
    };

    let slug = client(&server)
        .create_from_ai(&key(), &request)
        .await
        .unwrap();

    assert_eq!(slug.as_str(), "one-pan-lemon-orzo");
    let received = the_request(&server, "POST", "/api/recipes/create/ai").await;
    assert!(received.url.query().is_none());
    let parts = multipart(&received);
    let names: Vec<_> = parts.iter().map(|part| part.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "content",
            "images",
            "images",
            "translateLanguage",
            "createNewOrganizers"
        ]
    );
    assert!(!names.contains(&"url"));
    assert_eq!(parts[0].text(), request.content);
    assert_eq!(parts[1].file_name.as_deref(), Some("photo-1.JPG"));
    assert_eq!(parts[1].content_type.as_deref(), Some("image/jpeg"));
    assert_eq!(parts[1].body, binary(PHOTO_LEN));
    assert_eq!(parts[3].text(), "pt-BR");
    assert_eq!(parts[4].text(), "true");
}

#[tokio::test]
async fn create_from_ai_sends_no_language_or_photo_parts_when_there_are_none() {
    let server = serve(ResponseTemplate::new(201).set_body_json("soup")).await;
    client(&server)
        .create_from_ai(&key(), &ai_request("Caption: soup"))
        .await
        .unwrap();

    let received = the_request(&server, "POST", "/api/recipes/create/ai").await;
    let parts = multipart(&received);
    let sent: Vec<_> = parts
        .iter()
        .map(|part| (part.name.as_str(), part.text()))
        .collect();
    assert_eq!(
        sent,
        [
            ("content", "Caption: soup"),
            ("createNewOrganizers", "false")
        ]
    );
}

#[tokio::test]
async fn create_from_ai_rejects_a_body_that_is_not_a_usable_slug() {
    for body in [json!(""), json!({ "slug": "soup" }), json!("..")] {
        let server = serve(ResponseTemplate::new(201).set_body_json(&body)).await;
        let error = client(&server)
            .create_from_ai(&key(), &ai_request("x"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, MealieError::Other(Cause::Decode(_))),
            "{body}: {error:?}"
        );
    }
}

#[tokio::test]
async fn get_recipe_reads_what_the_pipeline_checks() {
    let server = serve(ResponseTemplate::new(200).set_body_json(recipe_json())).await;

    let recipe = client(&server)
        .get_recipe(&key(), &slug("one-pan-lemon-orzo"))
        .await
        .unwrap();

    assert_eq!(recipe.slug.as_str(), "one-pan-lemon-orzo");
    assert_eq!(recipe.name.as_deref(), Some("One-pan lemon orzo"));
    let ingredients: Vec<_> = recipe
        .recipe_ingredient
        .iter()
        .map(|i| i.display.as_str())
        .collect();
    assert_eq!(ingredients, ["2 tbsp butter", "1 cup orzo"]);
    assert_eq!(recipe.recipe_instructions[0].text, "Melt the butter.");
    assert!(!recipe.settings.unwrap().show_assets());
    the_request(&server, "GET", "/api/recipes/one-pan-lemon-orzo").await;
}

#[tokio::test]
async fn patch_recipe_sends_only_the_fields_it_sets() {
    let mut patched = recipe_json();
    patched["slug"] = json!("one-pan-lemon-orzo-2");
    let server = serve(ResponseTemplate::new(200).set_body_json(patched)).await;
    let patch = RecipePatch {
        org_url: Some("https://www.instagram.com/reel/C9xYz12AbCd/".to_owned()),
        settings: None,
    };

    let recipe = client(&server)
        .patch_recipe(&key(), &slug("one-pan-lemon-orzo"), &patch)
        .await
        .unwrap();

    assert_eq!(recipe.slug.as_str(), "one-pan-lemon-orzo-2");
    let request = the_request(&server, "PATCH", "/api/recipes/one-pan-lemon-orzo").await;
    assert_eq!(
        request.body_json::<Value>().unwrap(),
        json!({ "orgURL": "https://www.instagram.com/reel/C9xYz12AbCd/" })
    );
}

#[tokio::test]
async fn patch_recipe_sends_the_whole_settings_object_it_was_given() {
    let mut fetched = recipe_json();
    fetched["settings"]["aFlagMealieAddsLater"] = json!("kept");
    let server = serve(ResponseTemplate::new(200).set_body_json(&fetched)).await;
    let api = client(&server);
    let recipe = api
        .get_recipe(&key(), &slug("one-pan-lemon-orzo"))
        .await
        .unwrap();
    let settings: RecipeSettings = recipe.settings.unwrap().with_show_assets(true);

    api.patch_recipe(
        &key(),
        &recipe.slug,
        &RecipePatch {
            org_url: None,
            settings: Some(settings),
        },
    )
    .await
    .unwrap();

    let requests = server.received_requests().await.unwrap();
    let mut expected = fetched["settings"].clone();
    expected["showAssets"] = json!(true);
    assert_eq!(
        requests[1].body_json::<Value>().unwrap(),
        json!({ "settings": expected })
    );
}

#[tokio::test]
async fn delete_recipe_sends_a_delete() {
    let server = serve(ResponseTemplate::new(200).set_body_json(recipe_json())).await;

    client(&server)
        .delete_recipe(&key(), &slug("one-pan-lemon-orzo"))
        .await
        .unwrap();

    the_request(&server, "DELETE", "/api/recipes/one-pan-lemon-orzo").await;
}

#[tokio::test]
async fn upload_image_puts_the_file_and_its_extension() {
    let server = serve(ResponseTemplate::new(200).set_body_json(json!({ "image": "abc" }))).await;
    let files = Files::new();

    client(&server)
        .upload_image(&key(), &slug("one-pan-lemon-orzo"), &files.photo)
        .await
        .unwrap();

    let request = the_request(&server, "PUT", "/api/recipes/one-pan-lemon-orzo/image").await;
    let parts = multipart(&request);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].name, "image");
    assert_eq!(parts[0].file_name.as_deref(), Some("photo-1.JPG"));
    assert_eq!(parts[0].body, binary(PHOTO_LEN));
    assert_eq!(
        (parts[1].name.as_str(), parts[1].text()),
        ("extension", "jpg")
    );
}

#[tokio::test]
async fn upload_asset_posts_name_icon_extension_and_the_whole_file() {
    let template = ResponseTemplate::new(200)
        .set_body_json(json!({ "name": "video-1", "icon": "mdi-file", "fileName": "video-1.mp4" }));
    let server = serve(template).await;
    let files = Files::new();
    let asset = AssetUpload {
        name: "video-1".to_owned(),
        icon: AssetIcon::Video,
        path: files.video.clone(),
    };

    client(&server)
        .upload_asset(&key(), &slug("one-pan-lemon-orzo"), &asset)
        .await
        .unwrap();

    let request = the_request(&server, "POST", "/api/recipes/one-pan-lemon-orzo/assets").await;
    let parts = multipart(&request);
    let fields: Vec<_> = parts.iter().map(|part| part.name.as_str()).collect();
    assert_eq!(fields, ["name", "icon", "extension", "file"]);
    assert_eq!(parts[0].text(), "video-1");
    assert_eq!(parts[1].text(), "mdi-file");
    assert_eq!(parts[2].text(), "mp4");
    assert_eq!(parts[3].content_type.as_deref(), Some("video/mp4"));
    assert_eq!(parts[3].body, binary(VIDEO_LEN));
    assert!(
        request.headers.contains_key("content-length")
            && !request.headers.contains_key("transfer-encoding"),
        "a streamed part with a known length keeps the request a fixed size"
    );
}

#[tokio::test]
async fn photo_assets_use_the_photo_icon() {
    let server = serve(ResponseTemplate::new(200).set_body_json(json!({}))).await;
    let files = Files::new();
    let asset = AssetUpload {
        name: "photo-2".to_owned(),
        icon: AssetIcon::Photo,
        path: files.photo.clone(),
    };

    client(&server)
        .upload_asset(&key(), &slug("soup"), &asset)
        .await
        .unwrap();

    let request = the_request(&server, "POST", "/api/recipes/soup/assets").await;
    assert_eq!(multipart(&request)[1].text(), "mdi-file-image");
}

#[tokio::test]
async fn a_file_that_cannot_be_read_fails_before_anything_is_sent() {
    let server = serve(ResponseTemplate::new(200)).await;
    let files = Files::new();
    let missing = files.photo.with_file_name("gone.jpg");
    let no_extension = files.photo.with_file_name("photo");
    let api = client(&server);

    let image = api.upload_image(&key(), &slug("soup"), &missing).await;
    let asset = api
        .upload_asset(
            &key(),
            &slug("soup"),
            &AssetUpload {
                name: "x".to_owned(),
                icon: AssetIcon::Photo,
                path: no_extension,
            },
        )
        .await;

    assert!(
        matches!(image, Err(MealieError::Other(Cause::File(_)))),
        "{image:?}"
    );
    assert!(
        matches!(asset, Err(MealieError::Other(Cause::File(_)))),
        "{asset:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_key_that_cannot_be_a_header_is_unauthorized_and_sends_nothing() {
    let server = serve(ResponseTemplate::new(200).set_body_json(user_json())).await;

    let bad = secrecy::SecretString::from("key-with-newline\n");
    let error = client(&server).users_self(&bad).await.unwrap_err();

    assert!(matches!(error, MealieError::Unauthorized), "{error:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_base_path_is_kept_and_a_slash_in_a_slug_cannot_change_the_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/mealie/api/recipes/a%2Fb"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "slug": "a%2Fb" })))
        .mount(&server)
        .await;
    let api = client_for(&format!("{}/mealie", server.uri()), Duration::from_secs(10));

    let recipe = api.get_recipe(&key(), &slug("a/b")).await;

    assert!(recipe.is_ok(), "{recipe:?}");
}

#[tokio::test]
async fn the_client_works_behind_a_trait_object() {
    let server = serve(ResponseTemplate::new(200).set_body_json(user_json())).await;
    let api: Arc<dyn MealieApi> = Arc::new(client(&server));

    let user = tokio::spawn(async move { api.users_self(&key()).await })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(user.group_slug, "home");
}

#[tokio::test]
async fn every_call_sends_the_key_as_a_bearer_token() {
    let server = serve(ResponseTemplate::new(204)).await;
    let files = Files::new();
    let api = client(&server);

    for call in Call::ALL {
        let _ = call.run(&api, &files).await;
    }

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), Call::ALL.len());
    for request in &requests {
        assert_eq!(authorization(request), bearer(), "{}", request.url);
    }
}

#[tokio::test]
async fn every_call_maps_an_http_status_to_one_error() {
    let files = Files::new();
    let cases = [
        (400, "Rejected"),
        (401, "Unauthorized"),
        (403, "Unauthorized"),
        (404, "NotFound"),
        (409, "Other"),
        (422, "Other"),
        (429, "Other"),
        (500, "Unreachable"),
        (502, "Unreachable"),
        (503, "Unreachable"),
        (504, "Unreachable"),
    ];

    for (status, expected) in cases {
        let server = serve(ResponseTemplate::new(status)).await;
        let api = client(&server);
        for call in Call::ALL {
            let error = call.run(&api, &files).await.unwrap_err();
            assert_eq!(kind(&error), expected, "{call:?} with {status}: {error:?}");
        }
    }
}

#[tokio::test]
async fn a_rejected_request_carries_mealies_body_for_the_log() {
    let body = r#"{"detail":{"message":"Os serviços de IA não estão ativados","error":true}}"#;
    let server = serve(ResponseTemplate::new(400).set_body_string(body)).await;

    let error = client(&server).groups_self(&key()).await.unwrap_err();

    let MealieError::Rejected(rejected) = error else {
        panic!("a 400 is Rejected");
    };
    assert_eq!(rejected.as_str(), body);
}

#[tokio::test]
async fn every_call_that_gets_no_answer_in_time_is_unreachable() {
    let server = serve(ResponseTemplate::new(200).set_delay(Duration::from_millis(600))).await;
    let api = client_for(&server.uri(), Duration::from_millis(100));
    let files = Files::new();

    for call in Call::ALL {
        let error = call.run(&api, &files).await.unwrap_err();
        assert!(
            matches!(&error, MealieError::Unreachable(Cause::Transport(e)) if e.is_timeout()),
            "{call:?} gave {error:?}"
        );
    }
}

#[tokio::test]
async fn every_call_that_is_refused_is_unreachable() {
    let api = refused_client();
    let files = Files::new();

    for call in Call::ALL {
        let error = call.run(&api, &files).await.unwrap_err();
        assert!(
            matches!(&error, MealieError::Unreachable(Cause::Transport(e)) if e.is_connect()),
            "{call:?} gave {error:?}"
        );
    }
}

#[tokio::test]
async fn a_redirect_is_not_followed_so_the_key_stays_with_the_configured_mealie() {
    let server = MockServer::start().await;
    Mock::given(path("/api/users/self"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/elsewhere", server.uri())),
        )
        .mount(&server)
        .await;

    let error = client(&server).users_self(&key()).await.unwrap_err();

    assert!(
        matches!(error, MealieError::Other(Cause::Status(s)) if s.as_u16() == 302),
        "{error:?}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_redirect_is_logged_at_warn_with_the_status_and_without_the_query() {
    let logs = capture_logs();
    let server = serve(ResponseTemplate::new(301).insert_header(
        "location",
        "https://mealie.test/api/recipes/x?token=in-the-query",
    ))
    .await;

    client(&server)
        .get_recipe(&key(), &slug("redirect-warn-probe"))
        .await
        .unwrap_err();

    let logged = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    let line = logged
        .lines()
        .find(|line| line.contains("WARN") && line.contains("redirect-warn-probe"))
        .unwrap_or_else(|| panic!("no warn line for the redirect:\n{logged}"));
    assert!(line.contains("status=301"), "{line}");
    assert!(line.contains("MEALIE_URL"), "{line}");
    assert!(
        !line.contains("in-the-query") && !line.contains(KEY),
        "{line}"
    );
}

#[tokio::test]
async fn a_connection_dropped_mid_exchange_is_unreachable() {
    let replies: [&'static [u8]; 2] = [
        b"",
        b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\npartial",
    ];
    for reply in replies {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let _ = socket.read(&mut [0; 4096]);
            let _ = socket.write_all(reply);
        });
        let api = client_for(&format!("http://127.0.0.1:{port}"), Duration::from_secs(5));

        let error = api.users_self(&key()).await.unwrap_err();

        assert!(
            matches!(error, MealieError::Unreachable(Cause::Transport(_))),
            "{reply:?}: {error:?}"
        );
    }
}

#[tokio::test]
async fn a_body_the_client_cannot_read_is_a_decode_cause_not_a_transport_one() {
    let bodies = [
        ResponseTemplate::new(200).set_body_string("<html>a proxy login page</html>"),
        ResponseTemplate::new(200).set_body_json(json!({ "aiProviderSettings": "oops" })),
    ];
    for body in bodies {
        let server = serve(body).await;

        let error = client(&server).groups_self(&key()).await.unwrap_err();

        assert!(
            matches!(error, MealieError::Other(Cause::Decode(_))),
            "{error:?}"
        );
    }
}

#[tokio::test]
async fn from_config_uses_the_configured_url_and_timeout() {
    let server = serve(ResponseTemplate::new(200).set_delay(Duration::from_millis(2500))).await;
    let vars = HashMap::from([
        ("MEALIE_URL".to_owned(), server.uri()),
        ("MEALIE_TIMEOUT_SECS".to_owned(), "1".to_owned()),
    ]);
    let api = MealieClient::from_config(&Config::from_vars(&vars).unwrap()).unwrap();

    let error = api.users_self(&key()).await.unwrap_err();

    assert!(
        matches!(&error, MealieError::Unreachable(Cause::Transport(e)) if e.is_timeout()),
        "{error:?}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
