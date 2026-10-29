//! Both tests are `#[ignore]` and run by hand against a throwaway Mealie:
//!
//! ```sh
//! docker run -d --name mealie-live -p 127.0.0.1:19925:9000 ghcr.io/mealie-recipes/mealie:v3.29.0
//! MEALIE_LIVE_URL=http://127.0.0.1:19925 MEALIE_LIVE_KEY=<API token> \
//!   cargo test --test mealie live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! The second test points Mealie's AI at a fake LLM served from this process and
//! rewrites the group's AI settings, so it also needs `MEALIE_LIVE_ALLOW_AI_SETUP=1`.
//! The container reaches this process at `MEALIE_LIVE_LLM_HOST`, which defaults to
//! `host.docker.internal`.

use std::time::Duration;

use mealie_importer::mealie::{
    AiRecipeRequest, AssetIcon, AssetUpload, MealieApi, MealieClient, MealieError, RecipePatch,
};
use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{client_for, slug};

const FAKE_PROVIDER: &str = "importer-live-fake";

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x08, 0x02, 0x00, 0x00, 0x00, 0x4b, 0x6d, 0x29,
    0xdc, 0x00, 0x00, 0x00, 0x11, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x38, 0x51, 0xa1, 0x81,
    0x15, 0x31, 0x0c, 0x2d, 0x09, 0x00, 0x52, 0xd5, 0x5a, 0x01, 0x5f, 0x16, 0xe1, 0x04, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

struct Live {
    base: String,
    key: SecretString,
    raw: reqwest::Client,
    api: MealieClient,
}

impl Live {
    fn from_env() -> Self {
        let base = std::env::var("MEALIE_LIVE_URL").expect("MEALIE_LIVE_URL");
        let key = SecretString::from(std::env::var("MEALIE_LIVE_KEY").expect("MEALIE_LIVE_KEY"));
        let api = client_for(&base, Duration::from_secs(330));
        Self {
            base: base.trim_end_matches('/').to_owned(),
            key,
            raw: reqwest::Client::new(),
            api,
        }
    }

    async fn raw(&self, verb: Method, route: &str, body: Option<Value>) -> reqwest::Response {
        let mut request = self
            .raw
            .request(verb, format!("{}{route}", self.base))
            .bearer_auth(self.key.expose_secret());
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().await.expect("Mealie answers")
    }

    async fn raw_json(&self, verb: Method, route: &str, body: Option<Value>) -> Value {
        let response = self.raw(verb.clone(), route, body).await;
        let status = response.status();
        assert!(status.is_success(), "{verb} {route} gave {status}");
        response.json().await.unwrap_or(Value::Null)
    }

    async fn recipe_json(&self, slug: &str) -> Value {
        self.raw_json(Method::GET, &format!("/api/recipes/{slug}"), None)
            .await
    }

    async fn set_ai_providers(&self, settings: Value) {
        self.raw_json(
            Method::PUT,
            "/api/groups/ai-providers/settings",
            Some(settings),
        )
        .await;
    }

    /// A run that failed halfway leaves its provider behind, and Mealie refuses a second
    /// one with the same name. Mealie also refuses to delete a provider the settings use.
    async fn remove_fake_providers(&self) {
        let group = self.raw_json(Method::GET, "/api/groups/self", None).await;
        let providers = group["aiProviderSettings"]["providers"].as_array().unwrap();
        for provider in providers.iter().filter(|p| p["name"] == FAKE_PROVIDER) {
            self.set_ai_providers(json!({
                "defaultProviderId": null,
                "audioProviderId": null,
                "imageProviderId": null,
            }))
            .await;
            let id = provider["id"].as_str().unwrap();
            self.raw_json(
                Method::DELETE,
                &format!("/api/groups/ai-providers/providers/{id}"),
                None,
            )
            .await;
        }
    }
}

#[tokio::test]
#[ignore = "needs a running Mealie, see the module docs"]
async fn live_every_method_against_a_real_mealie() {
    let live = Live::from_env();
    let key = &live.key;
    let api = &live.api;
    let dir = tempfile::TempDir::new().unwrap();

    let user = api.users_self(key).await.unwrap();
    eprintln!("users_self: {user:?}");
    assert!(!user.id.is_empty() && !user.group_slug.is_empty());

    let wrong = api.users_self(&SecretString::from("not-a-key")).await;
    eprintln!("users_self with a wrong key: {wrong:?}");
    assert!(matches!(wrong, Err(MealieError::Unauthorized)));

    let group = api.groups_self(key).await.unwrap();
    eprintln!(
        "groups_self aiProviderSettings: {:?}",
        group.ai_provider_settings
    );

    let name = format!("Importer live test {}", std::process::id());
    let created = live
        .raw_json(Method::POST, "/api/recipes", Some(json!({ "name": name })))
        .await;
    let slug = slug(created.as_str().expect("the slug as a JSON string"));
    eprintln!("created {slug} through POST /api/recipes");

    let recipe = api.get_recipe(key, &slug).await.unwrap();
    assert_eq!(recipe.slug, slug);
    assert_eq!(recipe.name.as_deref(), Some(name.as_str()));
    assert!(recipe.settings.is_some());

    live.raw_json(
        Method::PATCH,
        &format!("/api/recipes/{slug}"),
        Some(json!({
            "recipeIngredient": [{ "note": "2 cups flour" }, { "note": "Could not detect ingredients" }],
            "recipeInstructions": [{ "text": "Mix well." }],
        })),
    )
    .await;
    let recipe = api.get_recipe(key, &slug).await.unwrap();
    let ingredients: Vec<_> = recipe
        .recipe_ingredient
        .iter()
        .map(|i| i.display.as_str())
        .collect();
    assert_eq!(
        ingredients,
        ["2 cups flour", "Could not detect ingredients"]
    );
    assert_eq!(recipe.recipe_instructions.len(), 1);
    assert_eq!(recipe.recipe_instructions[0].text, "Mix well.");

    let canonical = "https://www.instagram.com/reel/C9xYz12AbCd/";
    let patched = api
        .patch_recipe(
            key,
            &slug,
            &RecipePatch {
                org_url: Some(canonical.to_owned()),
                settings: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(patched.slug, slug);
    assert_eq!(live.recipe_json(slug.as_str()).await["orgURL"], canonical);
    assert_eq!(
        patched.recipe_ingredient.len(),
        2,
        "a partial PATCH leaves the rest alone"
    );

    let cover = dir.path().join("cover.png");
    std::fs::write(&cover, PNG).unwrap();
    api.upload_image(key, &slug, &cover).await.unwrap();
    let after_image = live.recipe_json(slug.as_str()).await;
    eprintln!("recipe image after upload_image: {}", after_image["image"]);
    assert!(after_image["image"].is_string());
    let recipe_id = after_image["id"].as_str().unwrap();
    let cover_file = live
        .raw(
            Method::GET,
            &format!("/api/media/recipes/{recipe_id}/images/original.webp"),
            None,
        )
        .await;
    assert!(cover_file.status().is_success(), "{}", cover_file.status());

    let clip = dir.path().join("clip.mp4");
    std::fs::write(&clip, vec![0_u8; 300_000]).unwrap();
    for (asset_name, icon, file) in [
        ("video-1", AssetIcon::Video, &clip),
        ("photo-2", AssetIcon::Photo, &cover),
    ] {
        let asset = AssetUpload {
            name: asset_name.to_owned(),
            icon,
            path: file.clone(),
        };
        api.upload_asset(key, &slug, &asset).await.unwrap();
    }
    let after_assets = live.recipe_json(slug.as_str()).await["assets"].clone();
    eprintln!("recipe assets after upload_asset: {after_assets}");
    assert_eq!(after_assets.as_array().unwrap().len(), 2);
    assert_eq!(after_assets[0]["icon"], "mdi-file");
    assert_eq!(after_assets[1]["icon"], "mdi-file-image");

    live.raw_json(
        Method::PATCH,
        &format!("/api/recipes/{slug}"),
        Some(
            json!({ "settings": { "public": true, "landscapeView": true, "showAssets": false,
            "showNutrition": false, "disableComments": false, "locked": false } }),
        ),
    )
    .await;
    let before = live.recipe_json(slug.as_str()).await["settings"].clone();
    let fetched = api.get_recipe(key, &slug).await.unwrap();
    let settings = fetched.settings.unwrap().with_show_assets(true);
    let patched = api
        .patch_recipe(
            key,
            &slug,
            &RecipePatch {
                org_url: None,
                settings: Some(settings),
            },
        )
        .await
        .unwrap();
    assert!(patched.settings.unwrap().show_assets());
    let after = live.recipe_json(slug.as_str()).await;
    let mut expected = before.clone();
    expected["showAssets"] = json!(true);
    assert_eq!(after["settings"], expected, "only showAssets changed");
    assert_eq!(
        after["assets"].as_array().unwrap().len(),
        2,
        "the assets survived the PATCH"
    );

    live.raw_json(
        Method::PATCH,
        &format!("/api/recipes/{slug}"),
        Some(json!({ "settings": { "showAssets": false } })),
    )
    .await;
    eprintln!(
        "settings after a partial settings PATCH of {{showAssets: false}}: {}",
        live.recipe_json(slug.as_str()).await["settings"]
    );

    let request = AiRecipeRequest {
        content: "Caption: lemon orzo".to_owned(),
        images: vec![],
        translate_language: Some("pt-BR".to_owned()),
        create_new_organizers: true,
    };
    let ai = api.create_from_ai(key, &request).await;
    eprintln!("create_from_ai: {ai:?}");
    if !group.ai_provider_settings.ai_enabled {
        assert!(matches!(ai, Err(MealieError::Rejected(_))));
    }

    api.delete_recipe(key, &slug).await.unwrap();
    assert!(matches!(
        api.get_recipe(key, &slug).await,
        Err(MealieError::NotFound)
    ));
    let again = api.delete_recipe(key, &slug).await;
    eprintln!("delete_recipe twice: {again:?}");
    assert!(matches!(again, Err(MealieError::NotFound)));
}

#[tokio::test]
#[ignore = "needs a running Mealie and MEALIE_LIVE_ALLOW_AI_SETUP=1, see the module docs"]
async fn live_create_from_ai_through_a_fake_llm() {
    assert_eq!(
        std::env::var("MEALIE_LIVE_ALLOW_AI_SETUP").as_deref(),
        Ok("1"),
        "this test rewrites the group's AI settings"
    );
    let live = Live::from_env();
    let key = &live.key;
    let api = &live.api;
    let dir = tempfile::TempDir::new().unwrap();

    // Mealie renames a recipe whose name is taken, so a leftover would break the check.
    let recipe_name = format!("Lemon orzo {}", std::process::id());
    let completion = json!({
        "id": "chatcmpl-live",
        "object": "chat.completion",
        "created": 0,
        "model": "fake-model",
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": {
                "role": "assistant",
                "content": json!({
                    "name": recipe_name,
                    "ingredients": [{ "text": "2 tbsp butter" }, { "text": "1 cup orzo" }],
                    "instructions": [{ "text": "Melt the butter." }, { "text": "Toast the orzo." }],
                }).to_string(),
            },
        }],
    });
    let llm = MockServer::builder()
        .listener(std::net::TcpListener::bind("0.0.0.0:0").unwrap())
        .start()
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion))
        .mount(&llm)
        .await;
    let host = std::env::var("MEALIE_LIVE_LLM_HOST").unwrap_or("host.docker.internal".to_owned());
    let llm_url = format!("http://{host}:{}/v1", llm.address().port());

    live.remove_fake_providers().await;
    let previous = live
        .raw_json(Method::GET, "/api/groups/ai-providers/settings", None)
        .await;
    let provider = live
        .raw_json(
            Method::POST,
            "/api/groups/ai-providers/providers",
            Some(json!({ "name": FAKE_PROVIDER, "baseUrl": llm_url, "apiKey": "fake", "model": "fake-model" })),
        )
        .await;
    let provider_id = provider["id"].as_str().unwrap().to_owned();
    live.set_ai_providers(json!({
        "defaultProviderId": provider_id,
        "audioProviderId": null,
        "imageProviderId": provider_id,
    }))
    .await;

    let group = api.groups_self(key).await.unwrap();
    eprintln!(
        "groups_self aiProviderSettings: {:?}",
        group.ai_provider_settings
    );
    assert!(group.ai_provider_settings.ai_enabled);
    assert!(group.ai_provider_settings.image_provider_enabled);

    let photo = dir.path().join("photo-1.png");
    std::fs::write(&photo, PNG).unwrap();
    let request = AiRecipeRequest {
        content: "Source: Instagram post by @cookwithmara\n\nCaption:\nOne-pan lemon orzo 🍋"
            .to_owned(),
        images: vec![photo],
        translate_language: Some("pt-BR".to_owned()),
        create_new_organizers: true,
    };
    let created = api.create_from_ai(key, &request).await;
    eprintln!("create_from_ai: {created:?}");
    let slug = created.expect("Mealie accepts the form");

    let recipe = api.get_recipe(key, &slug).await.unwrap();
    eprintln!(
        "recipe {} ingredients {:?} steps {:?}",
        recipe.slug,
        recipe
            .recipe_ingredient
            .iter()
            .map(|i| &i.display)
            .collect::<Vec<_>>(),
        recipe
            .recipe_instructions
            .iter()
            .map(|s| &s.text)
            .collect::<Vec<_>>(),
    );
    assert_eq!(recipe.name.as_deref(), Some(recipe_name.as_str()));
    assert_eq!(recipe.recipe_ingredient[0].display, "2 tbsp butter");
    assert_eq!(recipe.recipe_instructions[1].text, "Toast the orzo.");

    let sent = llm.received_requests().await.unwrap();
    eprintln!("the fake LLM got {} request(s)", sent.len());
    for (n, request) in sent.iter().enumerate() {
        let body: Value = request.body_json().unwrap();
        eprintln!(
            "  request {n}: {} bytes, system prompt starts {:?}",
            request.body.len(),
            body["messages"][0]["content"]
                .as_str()
                .map(|s| s.chars().take(70).collect::<String>())
        );
    }
    let reached = |needle: &str| {
        sent.iter()
            .any(|request| String::from_utf8_lossy(&request.body).contains(needle))
    };
    assert!(reached("cookwithmara"), "the content reached the LLM");
    assert!(
        reached("data:image/jpeg;base64,"),
        "the photo reached the LLM as an image"
    );
    assert!(reached("pt-BR"), "the translate language reached the LLM");

    api.delete_recipe(key, &slug).await.unwrap();
    live.remove_fake_providers().await;
    live.set_ai_providers(json!({
        "defaultProviderId": previous["defaultProviderId"],
        "audioProviderId": previous["audioProviderId"],
        "imageProviderId": previous["imageProviderId"],
    }))
    .await;
}
