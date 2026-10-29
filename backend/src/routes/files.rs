use axum::body::Body;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use crate::AppState;
use crate::error::AppError;
use crate::store::{FileError, FileName, ImportId};

#[derive(Deserialize)]
pub struct Link {
    exp: Option<String>,
    sig: Option<String>,
}

pub async fn download(
    State(state): State<AppState>,
    path: Result<Path<(String, String)>, PathRejection>,
    link: Result<Query<Link>, QueryRejection>,
) -> Result<Response, AppError> {
    let (id, name) = path
        .ok()
        .and_then(|Path((id, name))| {
            Some((id.parse::<ImportId>().ok()?, name.parse::<FileName>().ok()?))
        })
        .ok_or(AppError::NotFound)?;
    let (exp, sig) = link
        .ok()
        .and_then(|Query(link)| Some((link.exp?.parse::<i64>().ok()?, link.sig?)))
        .ok_or(AppError::Forbidden)?;
    let path = state
        .imports
        .signed_file(id, &name, exp, &sig)
        .map_err(|error| match error {
            FileError::Forbidden => AppError::Forbidden,
            FileError::NotFound => AppError::NotFound,
        })?;
    let file = tokio::fs::File::open(&path).await.map_err(|error| {
        tracing::warn!(%id, %name, %error, "cannot open a listed import file");
        AppError::NotFound
    })?;
    let bytes = file.metadata().await.map_err(|_| AppError::NotFound)?.len();
    Ok((
        [
            (header::CONTENT_TYPE, name.mime().to_owned()),
            (header::CONTENT_LENGTH, bytes.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response())
}
