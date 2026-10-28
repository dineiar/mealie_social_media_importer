use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use reqwest::multipart::{Form, Part};
use reqwest::{RequestBuilder, Response};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use url::Url;

use super::MealieApi;
use super::error::{Cause, ERROR_BODY_LIMIT, Failure, MealieError, classify};
use super::types::{
    AiRecipeRequest, AssetUpload, MealieGroup, MealieUser, Recipe, RecipePatch, RecipeSlug,
};
use crate::config::{BaseUrl, Config};

/// A host that drops packets should fail in seconds, not after the 330 s request timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct MealieClient {
    http: reqwest::Client,
    base: BaseUrl,
    lookup_timeout: Duration,
}

impl MealieClient {
    pub fn from_config(config: &Config) -> Result<Self, reqwest::Error> {
        Self::new(config.mealie_url.clone(), config.mealie_timeout)
    }

    /// Redirects are off so the key can only go to the configured Mealie, and a followed
    /// 301 would turn a POST into a GET.
    pub fn new(base: BaseUrl, timeout: Duration) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            base,
            lookup_timeout: timeout.min(LOOKUP_TIMEOUT),
        })
    }

    pub fn with_lookup_timeout(mut self, timeout: Duration) -> Self {
        self.lookup_timeout = timeout;
        self
    }

    async fn send(
        &self,
        key: &SecretString,
        request: RequestBuilder,
    ) -> Result<Response, MealieError> {
        let request = request
            .header(AUTHORIZATION, authorization(key)?)
            .build()
            .map_err(transport)?;
        let method = request.method().clone();
        let path = request.url().path().to_owned();

        let response = self.http.execute(request).await.map_err(transport)?;

        let status = response.status();
        tracing::debug!(%method, path, status = status.as_u16(), "mealie response");
        if status.is_success() {
            return Ok(response);
        }
        if status.is_redirection() {
            tracing::warn!(
                %method,
                path,
                status = status.as_u16(),
                "mealie answered with a redirect that the client does not follow, check MEALIE_URL"
            );
        }
        let body = error_body(response).await;
        Err(classify(Failure::Status(status, body)))
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        key: &SecretString,
        request: RequestBuilder,
    ) -> Result<T, MealieError> {
        self.send(key, request)
            .await?
            .json()
            .await
            .map_err(transport)
    }

    fn recipe_url(&self, slug: &RecipeSlug) -> Url {
        self.base.join(["api", "recipes", slug.as_str()])
    }
}

#[async_trait]
impl MealieApi for MealieClient {
    async fn users_self(&self, key: &SecretString) -> Result<MealieUser, MealieError> {
        let url = self.base.join(["api", "users", "self"]);
        self.send_json(key, self.http.get(url).timeout(self.lookup_timeout))
            .await
    }

    async fn groups_self(&self, key: &SecretString) -> Result<MealieGroup, MealieError> {
        let url = self.base.join(["api", "groups", "self"]);
        self.send_json(key, self.http.get(url).timeout(self.lookup_timeout))
            .await
    }

    async fn create_from_ai(
        &self,
        key: &SecretString,
        request: &AiRecipeRequest,
    ) -> Result<RecipeSlug, MealieError> {
        let mut form = Form::new().text("content", request.content.clone());
        for image in &request.images {
            form = form.part("images", file_part(image).await?);
        }
        if let Some(language) = &request.translate_language {
            form = form.text("translateLanguage", language.clone());
        }
        form = form.text(
            "createNewOrganizers",
            request.create_new_organizers.to_string(),
        );
        let url = self.base.join(["api", "recipes", "create", "ai"]);
        self.send_json(key, self.http.post(url).multipart(form))
            .await
    }

    async fn get_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
    ) -> Result<Recipe, MealieError> {
        let url = self.recipe_url(slug);
        self.send_json(key, self.http.get(url)).await
    }

    async fn patch_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        patch: &RecipePatch,
    ) -> Result<Recipe, MealieError> {
        let url = self.recipe_url(slug);
        self.send_json(key, self.http.patch(url).json(patch)).await
    }

    async fn delete_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
    ) -> Result<(), MealieError> {
        let url = self.recipe_url(slug);
        self.send(key, self.http.delete(url)).await?;
        Ok(())
    }

    async fn upload_image(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        image: &Path,
    ) -> Result<(), MealieError> {
        let form = Form::new()
            .part("image", file_part(image).await?)
            .text("extension", extension_of(image)?);
        let url = self.base.join(["api", "recipes", slug.as_str(), "image"]);
        self.send(key, self.http.put(url).multipart(form)).await?;
        Ok(())
    }

    async fn upload_asset(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        asset: &AssetUpload,
    ) -> Result<(), MealieError> {
        let form = Form::new()
            .text("name", asset.name.clone())
            .text("icon", asset.icon.mdi_name())
            .text("extension", extension_of(&asset.path)?)
            .part("file", file_part(&asset.path).await?);
        let url = self.base.join(["api", "recipes", slug.as_str(), "assets"]);
        self.send(key, self.http.post(url).multipart(form)).await?;
        Ok(())
    }
}

fn authorization(key: &SecretString) -> Result<HeaderValue, MealieError> {
    let mut value = HeaderValue::try_from(format!("Bearer {}", key.expose_secret()))
        .map_err(|_| MealieError::Unauthorized)?;
    value.set_sensitive(true);
    Ok(value)
}

fn transport(error: reqwest::Error) -> MealieError {
    classify(Failure::Transport(error.without_url()))
}

async fn error_body(mut response: Response) -> String {
    let mut body = Vec::new();
    while body.len() < ERROR_BODY_LIMIT {
        match response.chunk().await {
            Ok(Some(chunk)) => body.extend_from_slice(&chunk),
            Ok(None) | Err(_) => break,
        }
    }
    String::from_utf8_lossy(&body).into_owned()
}

async fn file_part(path: &Path) -> Result<Part, MealieError> {
    Part::file(path)
        .await
        .map_err(|error| MealieError::Other(Cause::File(error)))
}

fn extension_of(path: &Path) -> Result<String, MealieError> {
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            MealieError::Other(Cause::File(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file has no extension",
            )))
        })
}
