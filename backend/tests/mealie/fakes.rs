use std::path::Path;

use async_trait::async_trait;
use mealie_importer::mealie::{
    AiProviderSettings, AiRecipeRequest, AssetUpload, Cause, MealieApi, MealieError, MealieGroup,
    MealieUser, Recipe, RecipeIngredient, RecipePatch, RecipeSettings, RecipeSlug, RecipeStep,
    RejectedBody,
};
use reqwest::StatusCode;
use secrecy::SecretString;
use serde_json::{Value, json};

use crate::support::{ai_request, key, slug};

struct FakeMealie {
    ai_enabled: bool,
}

fn settings(show_assets: bool) -> RecipeSettings {
    let Value::Object(map) = json!({ "public": true, "showAssets": show_assets }) else {
        unreachable!("an object literal");
    };
    RecipeSettings::new(map)
}

fn recipe(slug: RecipeSlug) -> Recipe {
    Recipe {
        slug,
        name: Some("One-pan lemon orzo".to_owned()),
        recipe_ingredient: vec![RecipeIngredient {
            display: "1 cup orzo".to_owned(),
            note: None,
            original_text: Some("1 cup orzo".to_owned()),
        }],
        recipe_instructions: vec![RecipeStep {
            text: "Melt the butter.".to_owned(),
        }],
        settings: Some(settings(false)),
    }
}

#[async_trait]
impl MealieApi for FakeMealie {
    async fn users_self(&self, _: &SecretString) -> Result<MealieUser, MealieError> {
        Ok(MealieUser {
            id: "1".to_owned(),
            username: Some("admin".to_owned()),
            full_name: None,
            group_slug: "home".to_owned(),
        })
    }

    async fn groups_self(&self, _: &SecretString) -> Result<MealieGroup, MealieError> {
        Ok(MealieGroup {
            ai_provider_settings: AiProviderSettings {
                ai_enabled: self.ai_enabled,
                image_provider_enabled: false,
            },
        })
    }

    async fn create_from_ai(
        &self,
        _: &SecretString,
        _: &AiRecipeRequest,
    ) -> Result<RecipeSlug, MealieError> {
        if self.ai_enabled {
            Ok(slug("one-pan-lemon-orzo"))
        } else {
            Err(MealieError::Rejected(RejectedBody::new(
                "AI services are not enabled".to_owned(),
            )))
        }
    }

    async fn get_recipe(&self, _: &SecretString, slug: &RecipeSlug) -> Result<Recipe, MealieError> {
        Ok(recipe(slug.clone()))
    }

    async fn patch_recipe(
        &self,
        _: &SecretString,
        slug: &RecipeSlug,
        patch: &RecipePatch,
    ) -> Result<Recipe, MealieError> {
        Ok(Recipe {
            settings: patch.settings.clone(),
            ..recipe(slug.clone())
        })
    }

    async fn delete_recipe(&self, _: &SecretString, _: &RecipeSlug) -> Result<(), MealieError> {
        Err(MealieError::Unreachable(Cause::Status(
            StatusCode::BAD_GATEWAY,
        )))
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
        Err(MealieError::Unauthorized)
    }
}

#[tokio::test]
async fn code_outside_the_module_can_build_a_rejected_error() {
    let api: &dyn MealieApi = &FakeMealie { ai_enabled: false };
    let error = api
        .create_from_ai(&key(), &ai_request("Caption"))
        .await
        .unwrap_err();

    let MealieError::Rejected(body) = &error else {
        panic!("built as Rejected, got {error:?}");
    };
    assert_eq!(body.as_str(), "AI services are not enabled");
    assert_eq!(error.to_string(), "Mealie rejected the request");
}

#[tokio::test]
async fn a_fake_can_build_recipe_settings_without_serde() {
    let api: &dyn MealieApi = &FakeMealie { ai_enabled: true };
    let recipe = api
        .get_recipe(&key(), &slug("one-pan-lemon-orzo"))
        .await
        .unwrap();
    let before = recipe.settings.unwrap();
    assert!(!before.show_assets());

    let patch = RecipePatch {
        org_url: None,
        settings: Some(before.with_show_assets(true)),
    };
    let patched = api
        .patch_recipe(&key(), &slug("one-pan-lemon-orzo"), &patch)
        .await
        .unwrap();

    assert!(patched.settings.unwrap().show_assets());
}
