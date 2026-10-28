use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use moka::future::Cache;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};

use crate::mealie::{
    AiRecipeRequest, AssetUpload, MealieApi, MealieError, MealieGroup, MealieUser, Recipe,
    RecipePatch, RecipeSlug,
};

pub const USER_TTL: Duration = Duration::from_mins(5);

/// Only keys Mealie accepted get in, so this bounds memory, not abuse.
const MAX_CACHED_USERS: u64 = 1_000;

/// The cache holds the hash, never the key.
#[derive(Clone, PartialEq, Eq, Hash)]
struct KeyDigest([u8; 32]);

impl KeyDigest {
    fn of(key: &SecretString) -> Self {
        Self(Sha256::digest(key.expose_secret()).into())
    }
}

/// Mealie as seen on behalf of callers. It remembers which user a key belongs to and
/// forgets the key when any call made with it comes back `Unauthorized`.
///
/// `AppState` holds no other path to Mealie, so a handler cannot skip the eviction. A new
/// `MealieApi` method fails to compile here until it forwards through `evict_if_unauthorized`.
pub struct Authenticator {
    mealie: Arc<dyn MealieApi>,
    users: Cache<KeyDigest, MealieUser>,
}

impl Authenticator {
    pub fn new(mealie: Arc<dyn MealieApi>, ttl: Duration) -> Self {
        let users = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(MAX_CACHED_USERS)
            .build();
        Self { mealie, users }
    }

    /// Concurrent first requests with one key share a single `users/self` call. A failure is
    /// not cached.
    pub async fn user(&self, key: &SecretString) -> Result<MealieUser, Arc<MealieError>> {
        self.users
            .try_get_with(KeyDigest::of(key), self.users_self(key))
            .await
    }

    async fn evict_if_unauthorized<T>(
        &self,
        key: &SecretString,
        call: impl Future<Output = Result<T, MealieError>>,
    ) -> Result<T, MealieError> {
        let result = call.await;
        if matches!(result, Err(MealieError::Unauthorized)) {
            self.users.invalidate(&KeyDigest::of(key)).await;
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
