use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::file_name::FileName;
use super::wire::{ImportErrorKind, RecipeRef, Step, StepDetail, StepKey, StepState, WarningKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImportId(Uuid);

#[derive(Debug, thiserror::Error)]
#[error("not an import id")]
pub struct InvalidImportId;

impl ImportId {
    pub(super) fn time_ordered() -> Self {
        Self(Uuid::now_v7())
    }
}

impl FromStr for ImportId {
    type Err = InvalidImportId;

    fn from_str(value: &str) -> Result<Self, InvalidImportId> {
        Uuid::try_parse(value)
            .ok()
            .filter(|uuid| uuid.hyphenated().to_string() == value)
            .map(Self)
            .ok_or(InvalidImportId)
    }
}

impl fmt::Display for ImportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

/// The Mealie user id from `/api/users/self`, which scopes every import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(String);

impl From<String> for UserId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

pub struct NewImport {
    pub url: String,
    pub platform: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ImportRecord {
    pub(super) id: ImportId,
    pub(super) owner: UserId,
    pub(super) url: String,
    pub(super) platform: String,
    pub(super) language: Option<String>,
    pub(super) title: Option<String>,
    pub(super) steps: Steps,
    pub(super) warnings: Vec<WarningKind>,
    pub(super) files: Vec<StoredFile>,
    pub(super) created_at: Timestamp,
    #[serde(flatten)]
    pub(super) progress: Progress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum Progress {
    Queued,
    Running,
    Succeeded {
        recipe: RecipeRef,
        finished_at: Timestamp,
    },
    Failed {
        error: ImportErrorKind,
        finished_at: Timestamp,
    },
}

impl Progress {
    pub(super) fn finished_at(&self) -> Option<Timestamp> {
        match self {
            Self::Queued | Self::Running => None,
            Self::Succeeded { finished_at, .. } | Self::Failed { finished_at, .. } => {
                Some(*finished_at)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "Vec<Step>")]
pub(super) struct Steps([Step; 4]);

impl Steps {
    fn new() -> Self {
        Self(StepKey::ALL.map(|key| Step {
            key,
            state: StepState::Pending,
            detail: None,
        }))
    }

    pub(super) fn set(&mut self, key: StepKey, state: StepState, detail: Option<StepDetail>) {
        let step = self
            .0
            .iter_mut()
            .find(|step| step.key == key)
            .expect("every key has a step");
        *step = Step { key, state, detail };
    }

    pub(super) fn to_vec(&self) -> Vec<Step> {
        self.0.to_vec()
    }
}

impl TryFrom<Vec<Step>> for Steps {
    type Error = &'static str;

    fn try_from(steps: Vec<Step>) -> Result<Self, &'static str> {
        let steps: [Step; 4] = steps.try_into().map_err(|_| "there must be four steps")?;
        if steps.iter().map(|step| step.key).eq(StepKey::ALL) {
            Ok(Self(steps))
        } else {
            Err("the steps are not in pipeline order")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StoredFile {
    pub(super) name: FileName,
    pub(super) bytes: u64,
}

impl ImportRecord {
    pub(super) fn new(id: ImportId, owner: UserId, new: NewImport, now: Timestamp) -> Self {
        Self {
            id,
            owner,
            url: new.url,
            platform: new.platform,
            language: new.language,
            title: None,
            steps: Steps::new(),
            warnings: Vec::new(),
            files: Vec::new(),
            created_at: now,
            progress: Progress::Queued,
        }
    }

    pub(super) fn add_file(&mut self, name: FileName, bytes: u64) {
        match self.files.iter_mut().find(|file| file.name == name) {
            Some(file) => file.bytes = bytes,
            None => self.files.push(StoredFile { name, bytes }),
        }
    }

    pub(super) fn interrupt_if_unfinished(&mut self, now: Timestamp) -> bool {
        if self.progress.finished_at().is_some() {
            return false;
        }
        for step in &mut self.steps.0 {
            if step.state == StepState::Active {
                step.state = StepState::Failed;
            }
        }
        self.progress = Progress::Failed {
            error: ImportErrorKind::Interrupted,
            finished_at: now,
        };
        true
    }
}
