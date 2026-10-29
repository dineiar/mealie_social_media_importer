use std::collections::HashMap;
use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use mealie_importer::config::{BaseUrl, Config};
use mealie_importer::mealie::{
    AiRecipeRequest, AssetIcon, AssetUpload, MealieApi, MealieClient, MealieError, RecipePatch,
    RecipeSlug,
};
use secrecy::SecretString;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

pub const KEY: &str = "mk-9f3c1d7e-this-is-not-a-real-key";

type Logs = Arc<Mutex<Vec<u8>>>;

struct LogWriter(Logs);

impl io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The subscriber is global because a per-thread one loses events to tracing's callsite
/// cache whenever another test thread reaches a callsite first.
pub fn capture_logs() -> Logs {
    static LOGS: OnceLock<Logs> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Logs::default();
        let writer = logs.clone();
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || LogWriter(writer.clone()))
            .try_init()
            .expect("no other global subscriber or logger");
        logs
    })
    .clone()
}

pub fn key() -> SecretString {
    SecretString::from(KEY)
}

pub fn slug(value: &str) -> RecipeSlug {
    value.parse().expect("a usable slug")
}

pub fn ai_request(content: &str) -> AiRecipeRequest {
    AiRecipeRequest {
        content: content.to_owned(),
        images: Vec::new(),
        translate_language: None,
        create_new_organizers: false,
    }
}

pub fn config(url: &str) -> Config {
    let vars = HashMap::from([("MEALIE_URL".to_owned(), url.to_owned())]);
    Config::from_vars(&vars).expect("a valid URL")
}

pub fn base_url(url: &str) -> BaseUrl {
    config(url).mealie_url
}

pub fn client_for(url: &str, timeout: Duration) -> MealieClient {
    MealieClient::new(base_url(url), timeout).expect("client builds")
}

pub fn client(server: &MockServer) -> MealieClient {
    client_for(&server.uri(), Duration::from_secs(10))
}

pub fn refused_url() -> String {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    format!("http://127.0.0.1:{port}")
}

pub fn refused_client() -> MealieClient {
    client_for(&refused_url(), Duration::from_secs(10))
}

pub async fn calls_to(server: &MockServer, request_path: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == request_path)
        .count()
}

pub fn user_json() -> Value {
    json!({
        "id": "eaab0b0e-0df9-402c-8f5c-90cced537b01",
        "username": "admin",
        "fullName": "Change Me",
        "email": "changeme@example.com",
        "authMethod": "Mealie",
        "admin": true,
        "group": "Home",
        "household": "Family",
        "groupId": "6b1a1bee-d945-4637-91b4-9ba84955a398",
        "groupSlug": "home",
        "householdSlug": "family",
        "tokens": [{ "name": "importer", "id": 1 }],
    })
}

pub fn group_json(ai_enabled: bool, image_enabled: bool) -> Value {
    json!({
        "name": "Home",
        "slug": "home",
        "aiProviderSettings": {
            "defaultProviderId": null,
            "providers": [],
            "aiEnabled": ai_enabled,
            "audioProviderEnabled": false,
            "imageProviderEnabled": image_enabled,
        },
    })
}

pub fn users_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(user_json())
}

pub fn groups_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(group_json(true, false))
}

pub fn kind(error: &MealieError) -> &'static str {
    match error {
        MealieError::Unreachable(_) => "Unreachable",
        MealieError::Unauthorized => "Unauthorized",
        MealieError::Rejected(_) => "Rejected",
        MealieError::NotFound => "NotFound",
        MealieError::Other(_) => "Other",
    }
}

pub fn authorization(request: &Request) -> &str {
    request.headers["authorization"].to_str().unwrap()
}

pub fn bearer() -> String {
    format!("Bearer {KEY}")
}

pub async fn the_request(server: &MockServer, method: &str, path: &str) -> Request {
    let mut requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected exactly one request");
    let request = requests.remove(0);
    assert_eq!(authorization(&request), bearer());
    assert_eq!(
        (request.method.as_str(), request.url.path()),
        (method, path)
    );
    request
}

pub async fn serve(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

#[derive(Debug)]
pub struct Part {
    pub name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl Part {
    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.body).expect("a text part")
    }
}

pub fn multipart(request: &Request) -> Vec<Part> {
    let content_type = request.headers["content-type"].to_str().unwrap();
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("a multipart body");
    let delimiter = format!("--{boundary}");

    let mut chunks = split_on(&request.body, delimiter.as_bytes());
    chunks.remove(0);
    let mut parts = Vec::new();
    for chunk in chunks {
        if chunk.starts_with(b"--") {
            break;
        }
        let chunk = chunk
            .strip_prefix(b"\r\n")
            .expect("CRLF after the boundary");
        let split = find(chunk, b"\r\n\r\n").expect("a header block");
        let head = std::str::from_utf8(&chunk[..split]).expect("text headers");
        let body = chunk[split + 4..]
            .strip_suffix(b"\r\n")
            .expect("CRLF after the body");
        parts.push(parse_part(head, body));
    }
    parts
}

fn parse_part(head: &str, body: &[u8]) -> Part {
    let mut name = None;
    let mut file_name = None;
    let mut content_type = None;
    for line in head.split("\r\n") {
        if let Some(value) = line.strip_prefix("Content-Disposition: ") {
            for token in value.split("; ") {
                if let Some(value) = token.strip_prefix("name=\"") {
                    name = Some(value.trim_end_matches('"').to_owned());
                } else if let Some(value) = token.strip_prefix("filename=\"") {
                    file_name = Some(value.trim_end_matches('"').to_owned());
                }
            }
        } else if let Some(value) = line.strip_prefix("Content-Type: ") {
            content_type = Some(value.to_owned());
        }
    }
    Part {
        name: name.expect("a named part"),
        file_name,
        content_type,
        body: body.to_vec(),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn split_on<'a>(mut haystack: &'a [u8], needle: &[u8]) -> Vec<&'a [u8]> {
    let mut chunks = Vec::new();
    while let Some(at) = find(haystack, needle) {
        chunks.push(&haystack[..at]);
        haystack = &haystack[at + needle.len()..];
    }
    chunks.push(haystack);
    chunks
}

pub const VIDEO_LEN: usize = 100_000;
pub const PHOTO_LEN: usize = 2_000;

pub struct Files {
    _dir: TempDir,
    pub photo: PathBuf,
    pub video: PathBuf,
}

pub fn binary(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 256) as u8).collect()
}

impl Files {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let photo = dir.path().join("photo-1.JPG");
        let video = dir.path().join("video-1.mp4");
        std::fs::write(&photo, binary(PHOTO_LEN)).unwrap();
        std::fs::write(&video, binary(VIDEO_LEN)).unwrap();
        Self {
            _dir: dir,
            photo,
            video,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Call {
    UsersSelf,
    GroupsSelf,
    CreateFromAi,
    GetRecipe,
    PatchRecipe,
    DeleteRecipe,
    UploadImage,
    UploadAsset,
}

impl Call {
    pub const ALL: [Call; 8] = [
        Call::UsersSelf,
        Call::GroupsSelf,
        Call::CreateFromAi,
        Call::GetRecipe,
        Call::PatchRecipe,
        Call::DeleteRecipe,
        Call::UploadImage,
        Call::UploadAsset,
    ];

    pub async fn run(self, api: &dyn MealieApi, files: &Files) -> Result<(), MealieError> {
        let key = key();
        let slug = slug("one-pan-lemon-orzo");
        match self {
            Call::UsersSelf => api.users_self(&key).await.map(drop),
            Call::GroupsSelf => api.groups_self(&key).await.map(drop),
            Call::CreateFromAi => {
                let request = AiRecipeRequest {
                    images: vec![files.photo.clone()],
                    translate_language: Some("pt-BR".to_owned()),
                    create_new_organizers: true,
                    ..ai_request("Caption: lemon orzo")
                };
                api.create_from_ai(&key, &request).await.map(drop)
            }
            Call::GetRecipe => api.get_recipe(&key, &slug).await.map(drop),
            Call::PatchRecipe => {
                let patch = RecipePatch {
                    org_url: Some("https://www.instagram.com/reel/C9xYz12AbCd/".to_owned()),
                    settings: None,
                };
                api.patch_recipe(&key, &slug, &patch).await.map(drop)
            }
            Call::DeleteRecipe => api.delete_recipe(&key, &slug).await,
            Call::UploadImage => api.upload_image(&key, &slug, &files.photo).await,
            Call::UploadAsset => {
                let asset = AssetUpload {
                    name: "video-1".to_owned(),
                    icon: AssetIcon::Video,
                    path: files.video.clone(),
                };
                api.upload_asset(&key, &slug, &asset).await
            }
        }
    }
}
