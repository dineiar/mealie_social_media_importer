use axum::Json;
use axum::extract::State;
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;
use crate::auth::Caller;
use crate::platforms::{PlatformStatus, SetupHint};

#[derive(Serialize, TS)]
#[ts(export)]
pub struct PlatformInfo {
    id: &'static str,
    name: &'static str,
    status: PlatformState,
    hint: Option<SetupHint>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum PlatformState {
    Ready,
    NotConfigured,
}

pub async fn platforms(State(state): State<AppState>, _caller: Caller) -> Json<Vec<PlatformInfo>> {
    let mut infos = Vec::new();
    for platform in state.platforms.iter() {
        let (status, hint) = match platform.status().await {
            PlatformStatus::Ready => (PlatformState::Ready, None),
            PlatformStatus::NotConfigured(hint) => (PlatformState::NotConfigured, Some(hint)),
        };
        infos.push(PlatformInfo {
            id: platform.id(),
            name: platform.name(),
            status,
            hint,
        });
    }
    Json(infos)
}
