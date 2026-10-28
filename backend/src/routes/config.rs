use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct PublicConfig {
    mealie_public_url: String,
    retention_minutes: u64,
    version: &'static str,
}

pub async fn config(State(state): State<AppState>) -> Json<PublicConfig> {
    Json(PublicConfig {
        mealie_public_url: state.config.mealie_public_url.to_string(),
        retention_minutes: state.config.media_retention.as_secs() / 60,
        version: env!("CARGO_PKG_VERSION"),
    })
}
