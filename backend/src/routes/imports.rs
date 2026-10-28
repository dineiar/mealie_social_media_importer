use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};

use crate::AppState;
use crate::auth::Caller;
use crate::error::AppError;
use crate::store::{Import, UserId};

pub async fn list(State(state): State<AppState>, caller: Caller) -> Json<Vec<Import>> {
    Json(state.imports.list(&UserId::from(caller.user.id)))
}

pub async fn get(
    State(state): State<AppState>,
    caller: Caller,
    id: Result<Path<String>, PathRejection>,
) -> Result<Json<Import>, AppError> {
    let id = id
        .ok()
        .and_then(|Path(id)| id.parse().ok())
        .ok_or(AppError::NotFound)?;
    state
        .imports
        .get(&UserId::from(caller.user.id), id)
        .map(Json)
        .ok_or(AppError::NotFound)
}
