use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::file_name::FileName;
use super::record::{ImportId, ImportRecord, Progress};
use crate::config::BaseUrl;
use crate::mealie::RecipeSlug;

/// `error` is set only when `status` is `failed`, and `recipe` only when it is `succeeded`.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Import {
    #[ts(type = "string")]
    id: ImportId,
    url: String,
    platform: String,
    language: Option<String>,
    status: ImportStatus,
    title: Option<String>,
    steps: Vec<Step>,
    error: Option<ImportError>,
    warnings: Vec<Warning>,
    recipe: Option<RecipeRef>,
    files: Vec<ImportFile>,
    #[ts(type = "string")]
    created_at: Timestamp,
    #[ts(type = "string | null")]
    finished_at: Option<Timestamp>,
    #[ts(type = "string | null")]
    expires_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Step {
    pub(super) key: StepKey,
    pub(super) state: StepState,
    pub(super) detail: Option<StepDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StepKey {
    Download,
    Transcribe,
    CreateRecipe,
    AttachMedia,
}

impl StepKey {
    pub const ALL: [Self; 4] = [
        Self::Download,
        Self::Transcribe,
        Self::CreateRecipe,
        Self::AttachMedia,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Active,
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum StepDetail {
    Download(DownloadDetail),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DownloadDetail {
    pub media: MediaKind,
    pub bytes: u64,
    pub duration_s: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Video,
    Image,
}

#[derive(Debug, Serialize, TS)]
pub struct ImportError {
    kind: ImportErrorKind,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ImportErrorKind {
    NoRecipe,
    MealieUnreachable,
    MealieRejected,
    DownloadFailed,
    Interrupted,
}

impl ImportErrorKind {
    const fn message(self) -> &'static str {
        match self {
            Self::NoRecipe => "Mealie found no recipe in the post.",
            Self::MealieUnreachable => "Mealie did not respond.",
            Self::MealieRejected => "Mealie refused the import.",
            Self::DownloadFailed => "The post could not be downloaded.",
            Self::Interrupted => "The import stopped before it finished.",
        }
    }
}

#[derive(Debug, Serialize, TS)]
pub struct Warning {
    kind: WarningKind,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    TranscriptionFailed,
    AttachFailed,
}

impl WarningKind {
    const fn message(self) -> &'static str {
        match self {
            Self::TranscriptionFailed => "The audio could not be transcribed.",
            Self::AttachFailed => "The media could not be attached to the recipe.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecipeRef {
    slug: String,
    name: String,
    url: String,
}

impl RecipeRef {
    pub fn new(
        mealie_public_url: &BaseUrl,
        group_slug: &str,
        slug: &RecipeSlug,
        name: String,
    ) -> Self {
        let url = mealie_public_url.join(["g", group_slug, "r", slug.as_str()]);
        Self {
            slug: slug.to_string(),
            name,
            url: url.to_string(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
pub struct ImportFile {
    kind: FileKind,
    name: String,
    mime: &'static str,
    bytes: u64,
    url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Video,
    Image,
    Text,
}

impl Import {
    pub(super) fn new(
        record: &ImportRecord,
        expires_at: Option<Timestamp>,
        link: impl Fn(&FileName) -> String,
    ) -> Self {
        let (status, error, recipe, finished_at) = match &record.progress {
            Progress::Queued => (ImportStatus::Queued, None, None, None),
            Progress::Running => (ImportStatus::Running, None, None, None),
            Progress::Succeeded {
                recipe,
                finished_at,
            } => (
                ImportStatus::Succeeded,
                None,
                Some(recipe.clone()),
                Some(*finished_at),
            ),
            Progress::Failed { error, finished_at } => (
                ImportStatus::Failed,
                Some(ImportError {
                    kind: *error,
                    message: error.message(),
                }),
                None,
                Some(*finished_at),
            ),
        };
        Self {
            id: record.id,
            url: record.url.clone(),
            platform: record.platform.clone(),
            language: record.language.clone(),
            status,
            title: record.title.clone(),
            steps: record.steps.to_vec(),
            error,
            warnings: record
                .warnings
                .iter()
                .map(|&kind| Warning {
                    kind,
                    message: kind.message(),
                })
                .collect(),
            recipe,
            files: record
                .files
                .iter()
                .map(|file| ImportFile {
                    kind: file.name.kind(),
                    name: file.name.to_string(),
                    mime: file.name.mime(),
                    bytes: file.bytes,
                    url: link(&file.name),
                })
                .collect(),
            created_at: record.created_at,
            finished_at,
            expires_at,
        }
    }
}
