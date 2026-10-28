use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use moka::future::Cache;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::error::AppError;
use crate::mealie::{
    AiRecipeRequest, AssetUpload, MealieApi, MealieError, MealieGroup, MealieUser, Recipe,
    RecipePatch, RecipeSlug,
};

pub const USER_TTL: Duration = Duration::from_mins(5);

const MAX_CACHED_USERS: u64 = 1_000;

pub struct Caller {
    pub user: MealieUser,
    pub key: SecretString,
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let key = bearer_key(&parts.headers).ok_or(AppError::Unauthorized)?;
        let user = state
            .mealie
            .user(&key)
            .await
            .map_err(|error| AppError::from_identity_lookup(&error))?;
        Ok(Self { user, key })
    }
}

fn bearer_key(headers: &HeaderMap) -> Option<SecretString> {
    let (scheme, key) = headers.get(AUTHORIZATION)?.to_str().ok()?.split_once(' ')?;
    let key = key.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !key.is_empty()).then(|| SecretString::from(key))
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct KeyDigest([u8; 32]);

impl KeyDigest {
    fn of(key: &SecretString) -> Self {
        Self(Sha256::digest(key.expose_secret()).into())
    }
}

#[derive(Clone)]
struct Lookup {
    user: MealieUser,
    started: u64,
}

struct EvictionLog {
    clock: AtomicU64,
    evicted_at: Cache<KeyDigest, u64>,
}

impl EvictionLog {
    fn new(ttl: Duration) -> Self {
        Self {
            clock: AtomicU64::new(0),
            evicted_at: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(MAX_CACHED_USERS)
                .build(),
        }
    }

    fn now(&self) -> u64 {
        self.clock.load(Ordering::SeqCst)
    }

    async fn record(&self, digest: &KeyDigest) {
        let at = self.clock.fetch_add(1, Ordering::SeqCst) + 1;
        self.evicted_at.insert(digest.clone(), at).await;
    }

    async fn since(&self, digest: &KeyDigest, started: u64) -> bool {
        self.now() != started
            && self
                .evicted_at
                .get(digest)
                .await
                .is_some_and(|at| at > started)
    }
}

pub struct Authenticator {
    mealie: Arc<dyn MealieApi>,
    users: Cache<KeyDigest, Lookup>,
    evictions: EvictionLog,
}

impl Authenticator {
    pub fn new(mealie: Arc<dyn MealieApi>, ttl: Duration) -> Self {
        Self {
            mealie,
            users: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(MAX_CACHED_USERS)
                .build(),
            evictions: EvictionLog::new(ttl),
        }
    }

    pub async fn user(&self, key: &SecretString) -> Result<MealieUser, Arc<MealieError>> {
        let digest = KeyDigest::of(key);
        let lookup = self
            .users
            .try_get_with(digest.clone(), self.lookup(key))
            .await?;
        if self.evictions.since(&digest, lookup.started).await {
            self.users.invalidate(&digest).await;
        }
        Ok(lookup.user)
    }

    async fn lookup(&self, key: &SecretString) -> Result<Lookup, MealieError> {
        let started = self.evictions.now();
        let user = self.users_self(key).await?;
        Ok(Lookup { user, started })
    }

    async fn evict_if_unauthorized<T>(
        &self,
        key: &SecretString,
        call: impl Future<Output = Result<T, MealieError>>,
    ) -> Result<T, MealieError> {
        let result = call.await;
        if matches!(result, Err(MealieError::Unauthorized)) {
            let digest = KeyDigest::of(key);
            self.evictions.record(&digest).await;
            self.users.invalidate(&digest).await;
        }
        result
    }
}

#[async_trait]
impl MealieApi for Authenticator {
    async fn users_self(&self, key: &SecretString) -> Result<MealieUser, MealieError> {
        self.evict_if_unauthorized(key, self.mealie.users_self(key))
            .await
    }

    async fn groups_self(&self, key: &SecretString) -> Result<MealieGroup, MealieError> {
        self.evict_if_unauthorized(key, self.mealie.groups_self(key))
            .await
    }

    async fn create_from_ai(
        &self,
        key: &SecretString,
        request: &AiRecipeRequest,
    ) -> Result<RecipeSlug, MealieError> {
        self.evict_if_unauthorized(key, self.mealie.create_from_ai(key, request))
            .await
    }

    async fn get_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
    ) -> Result<Recipe, MealieError> {
        self.evict_if_unauthorized(key, self.mealie.get_recipe(key, slug))
            .await
    }

    async fn patch_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        patch: &RecipePatch,
    ) -> Result<Recipe, MealieError> {
        self.evict_if_unauthorized(key, self.mealie.patch_recipe(key, slug, patch))
            .await
    }

    async fn delete_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
    ) -> Result<(), MealieError> {
        self.evict_if_unauthorized(key, self.mealie.delete_recipe(key, slug))
            .await
    }

    async fn upload_image(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        image: &Path,
    ) -> Result<(), MealieError> {
        self.evict_if_unauthorized(key, self.mealie.upload_image(key, slug, image))
            .await
    }

    async fn upload_asset(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        asset: &AssetUpload,
    ) -> Result<(), MealieError> {
        self.evict_if_unauthorized(key, self.mealie.upload_asset(key, slug, asset))
            .await
    }
}
