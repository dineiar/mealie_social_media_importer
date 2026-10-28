use std::io;
use std::path::Path;
use std::time::Duration;

use reqwest::multipart::{Form, Part};
use reqwest::{Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use url::Url;

use crate::config::Transcription;

/// A host that drops packets should fail in seconds, not after the request timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A transcript of hours of speech is a few hundred KB. A larger reply is not one.
const REPLY_LIMIT: usize = 1024 * 1024;

/// Servers read the audio format from the extension.
const UPLOAD_NAME: &str = "audio.mp3";

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("the speech API did not respond")]
    Unreachable(#[source] reqwest::Error),
    #[error("the speech API answered with status {0}")]
    Status(StatusCode),
    #[error("the speech API's reply holds no transcript")]
    BadReply,
    #[error("the audio file could not be read")]
    File(#[source] io::Error),
    /// A key with a character that cannot go in an HTTP header ends up here.
    #[error("the speech API request could not be built")]
    Build(#[source] reqwest::Error),
}

#[derive(Debug)]
pub struct SpeechClient {
    http: reqwest::Client,
    url: Url,
    model: String,
    language: Option<String>,
    api_key: Option<SecretString>,
}

impl SpeechClient {
    /// Redirects are off so the key can only go to the configured server, and a followed
    /// 301 would turn the POST into a GET.
    pub fn new(speech: &Transcription, timeout: Duration) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            url: speech.base_url.join(["audio", "transcriptions"]),
            model: speech.model.clone(),
            language: speech.language.clone(),
            api_key: speech
                .api_key
                .as_ref()
                .map(|key| SecretString::from(key.expose_secret())),
        })
    }

    pub async fn transcribe(&self, audio: &Path) -> Result<String, SpeechError> {
        let bytes = tokio::fs::read(audio).await.map_err(SpeechError::File)?;
        let file = Part::bytes(bytes)
            .file_name(UPLOAD_NAME)
            .mime_str("audio/mpeg")
            .expect("a fixed MIME type parses");
        let mut form = Form::new()
            .part("file", file)
            .text("model", self.model.clone());
        if let Some(language) = &self.language {
            form = form.text("language", language.clone());
        }
        form = form.text("response_format", "json");

        let mut request = self.http.post(self.url.clone()).multipart(form);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key.expose_secret());
        }
        let response = request.send().await.map_err(transport)?;

        let status = response.status();
        tracing::debug!(status = status.as_u16(), "speech API response");
        if !status.is_success() {
            return Err(SpeechError::Status(status));
        }
        let body = read_capped(response).await?;
        parse_reply(&body)
    }
}

fn transport(error: reqwest::Error) -> SpeechError {
    let error = error.without_url();
    if error.is_builder() {
        SpeechError::Build(error)
    } else {
        SpeechError::Unreachable(error)
    }
}

async fn read_capped(mut response: Response) -> Result<Vec<u8>, SpeechError> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if body.len() + chunk.len() > REPLY_LIMIT {
            return Err(SpeechError::BadReply);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// The body is read as a generic value because a derived struct would also accept the
/// array `["..."]`.
fn parse_reply(body: &[u8]) -> Result<String, SpeechError> {
    let reply: Value = serde_json::from_slice(body).map_err(|_| SpeechError::BadReply)?;
    reply
        .get("text")
        .and_then(Value::as_str)
        .map(|text| text.trim().to_owned())
        .ok_or(SpeechError::BadReply)
}
