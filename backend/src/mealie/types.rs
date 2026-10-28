use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The slug names a recipe in every later call. An empty slug or a dot segment would
/// make `BaseUrl::join` drop it from the path and hit a collection endpoint instead.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct RecipeSlug(String);

#[derive(Debug, thiserror::Error)]
#[error("not a usable recipe slug")]
pub struct InvalidSlug;

impl RecipeSlug {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RecipeSlug {
    type Err = InvalidSlug;

    fn from_str(value: &str) -> Result<Self, InvalidSlug> {
        if matches!(value, "" | "." | "..") {
            Err(InvalidSlug)
        } else {
            Ok(Self(value.to_owned()))
        }
    }
}

impl TryFrom<String> for RecipeSlug {
    type Error = InvalidSlug;

    fn try_from(value: String) -> Result<Self, InvalidSlug> {
        value.parse()
    }
}

impl fmt::Display for RecipeSlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MealieUser {
    pub id: String,
    pub username: Option<String>,
    pub full_name: Option<String>,
    pub group_slug: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MealieGroup {
    pub ai_provider_settings: AiProviderSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderSettings {
    pub ai_enabled: bool,
    pub image_provider_enabled: bool,
}

/// There is no `url` field. Mealie would fetch the post itself, without cookies.
#[derive(Debug, Clone)]
pub struct AiRecipeRequest {
    pub content: String,
    pub images: Vec<PathBuf>,
    pub translate_language: Option<String>,
    pub create_new_organizers: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    pub slug: RecipeSlug,
    pub name: Option<String>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub recipe_ingredient: Vec<RecipeIngredient>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub recipe_instructions: Vec<RecipeStep>,
    pub settings: Option<RecipeSettings>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeIngredient {
    #[serde(default)]
    pub display: String,
    pub note: Option<String>,
    pub original_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeStep {
    pub text: String,
}

/// Mealie resets any setting a PATCH leaves out, so the object travels whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipeSettings(Map<String, Value>);

impl RecipeSettings {
    pub fn show_assets(&self) -> bool {
        self.0.get("showAssets").and_then(Value::as_bool) == Some(true)
    }

    pub fn with_show_assets(mut self, show: bool) -> Self {
        self.0.insert("showAssets".to_owned(), Value::Bool(show));
        self
    }
}

/// Has no `assets` field because `assets: []` makes Mealie delete the attached files.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RecipePatch {
    #[serde(rename = "orgURL", skip_serializing_if = "Option::is_none")]
    pub org_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<RecipeSettings>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetIcon {
    Photo,
    Video,
}

impl AssetIcon {
    pub(super) fn mdi_name(self) -> &'static str {
        match self {
            Self::Photo => "mdi-file-image",
            Self::Video => "mdi-file",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetUpload {
    pub name: String,
    pub icon: AssetIcon,
    pub path: PathBuf,
}

fn null_as_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn slug_rejects_what_would_leave_the_path() {
        for bad in ["", ".", ".."] {
            assert!(bad.parse::<RecipeSlug>().is_err(), "{bad:?}");
        }
        assert_eq!(
            "one-pan-lemon-orzo".parse::<RecipeSlug>().unwrap().as_str(),
            "one-pan-lemon-orzo"
        );
    }

    #[test]
    fn settings_round_trip_keeps_every_key() {
        let original = json!({
            "public": true,
            "showNutrition": false,
            "showAssets": false,
            "landscapeView": true,
            "disableComments": true,
            "locked": false,
            "aFlagMealieAddsLater": "kept",
        });
        let settings: RecipeSettings = serde_json::from_value(original.clone()).unwrap();

        let updated = serde_json::to_value(settings.with_show_assets(true)).unwrap();

        let mut expected = original;
        expected["showAssets"] = json!(true);
        assert_eq!(updated, expected);
    }

    #[test]
    fn patch_leaves_out_what_it_does_not_set() {
        let only_url = RecipePatch {
            org_url: Some("https://example.test/p/1".to_owned()),
            settings: None,
        };

        assert_eq!(
            serde_json::to_value(only_url).unwrap(),
            json!({ "orgURL": "https://example.test/p/1" })
        );
        assert_eq!(
            serde_json::to_value(RecipePatch::default()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn recipe_accepts_null_for_the_fields_mealie_makes_nullable() {
        let recipe: Recipe = serde_json::from_value(json!({
            "slug": "soup",
            "name": null,
            "recipeIngredient": null,
            "recipeInstructions": null,
            "settings": null,
        }))
        .unwrap();

        assert!(recipe.recipe_ingredient.is_empty());
        assert!(recipe.recipe_instructions.is_empty());
        assert!(recipe.name.is_none());
        assert!(recipe.settings.is_none());
    }

    #[test]
    fn recipe_with_an_unusable_slug_is_a_decode_error() {
        assert!(serde_json::from_value::<Recipe>(json!({ "slug": "" })).is_err());
    }
}
