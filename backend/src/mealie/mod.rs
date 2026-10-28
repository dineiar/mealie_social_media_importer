mod client;
mod error;
mod types;

use std::path::Path;

use async_trait::async_trait;
use secrecy::SecretString;

pub use client::MealieClient;
pub use error::{Cause, MealieError, RejectedBody};
pub use types::{
    AiProviderSettings, AiRecipeRequest, AssetIcon, AssetUpload, InvalidSlug, MealieGroup,
    MealieUser, Recipe, RecipeIngredient, RecipePatch, RecipeSettings, RecipeSlug, RecipeStep,
};

/// The key comes with each call because one client serves every user.
#[async_trait]
pub trait MealieApi: Send + Sync {
    async fn users_self(&self, key: &SecretString) -> Result<MealieUser, MealieError>;

    async fn groups_self(&self, key: &SecretString) -> Result<MealieGroup, MealieError>;

    /// Returns the slug of the recipe Mealie's AI wrote.
    async fn create_from_ai(
        &self,
        key: &SecretString,
        request: &AiRecipeRequest,
    ) -> Result<RecipeSlug, MealieError>;

    async fn get_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
    ) -> Result<Recipe, MealieError>;

    /// Returns the updated recipe. Take its slug from here on, since Mealie can change it.
    async fn patch_recipe(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        patch: &RecipePatch,
    ) -> Result<Recipe, MealieError>;

    async fn delete_recipe(&self, key: &SecretString, slug: &RecipeSlug)
    -> Result<(), MealieError>;

    /// Sets the recipe's cover. The extension comes from the file name.
    async fn upload_image(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        image: &Path,
    ) -> Result<(), MealieError>;

    /// Streams the file from disk. The extension comes from the file name.
    async fn upload_asset(
        &self,
        key: &SecretString,
        slug: &RecipeSlug,
        asset: &AssetUpload,
    ) -> Result<(), MealieError>;
}
