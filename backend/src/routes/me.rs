use axum::Json;
use axum::extract::State;
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;
use crate::auth::Caller;
use crate::error::AppError;
use crate::mealie::MealieApi;

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Me {
    id: String,
    username: Option<String>,
    full_name: Option<String>,
    group_slug: String,
    ai: AiStatus,
}

#[derive(Serialize, TS)]
struct AiStatus {
    enabled: bool,
    image_enabled: bool,
}

pub async fn me(State(state): State<AppState>, caller: Caller) -> Result<Json<Me>, AppError> {
    let group = state
        .mealie
        .groups_self(&caller.key)
        .await
        .map_err(|error| AppError::from_identity_lookup(&error))?;
    let ai = group.ai_provider_settings;
    let user = caller.user;
    Ok(Json(Me {
        id: user.id,
        username: user.username,
        full_name: user.full_name,
        group_slug: user.group_slug,
        ai: AiStatus {
            enabled: ai.ai_enabled,
            image_enabled: ai.image_provider_enabled,
        },
    }))
}
