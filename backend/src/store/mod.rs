mod file_name;
mod link;
mod record;
mod wire;

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};

pub use file_name::{FileName, InvalidFileName};
pub use record::{ImportId, InvalidImportId, NewImport, UserId};
pub use wire::{
    DownloadDetail, FileKind, Import, ImportError, ImportErrorKind, ImportFile, ImportStatus,
    MediaKind, RecipeRef, Step, StepDetail, StepKey, StepState, Warning, WarningKind,
};

use link::Signer;
use record::{ImportRecord, Progress};

pub const SWEEP_PERIOD: Duration = Duration::from_secs(60);

const MANIFEST: &str = "manifest.json";
const MANIFEST_TMP: &str = "manifest.json.tmp";

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

/// Every import, in memory and in `DATA_DIR/imports/<id>/`.
pub struct ImportStore {
    dir: PathBuf,
    retention: SignedDuration,
    clock: Arc<dyn Clock>,
    signer: Signer,
    imports: Mutex<HashMap<ImportId, ImportRecord>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FileError {
    Forbidden,
    NotFound,
}

impl ImportStore {
    /// An empty store that hasn't read `DATA_DIR/imports/`. Use `open` to serve.
    pub fn new(data_dir: &Path, retention: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            dir: data_dir.join("imports"),
            retention: SignedDuration::try_from(retention).unwrap_or(SignedDuration::MAX),
            clock,
            signer: Signer::random(),
            imports: Mutex::default(),
        }
    }

    /// Reloads `DATA_DIR/imports/`. Imports that were queued or running fail with
    /// `interrupted`, and expired ones and ones whose manifest can't be parsed are deleted.
    pub fn open(
        data_dir: &Path,
        retention: Duration,
        clock: Arc<dyn Clock>,
    ) -> io::Result<Arc<Self>> {
        let store = Self::new(data_dir, retention, clock);
        store.reload()?;
        Ok(Arc::new(store))
    }

    pub fn create(self: &Arc<Self>, owner: UserId, new: NewImport) -> io::Result<ImportWriter> {
        let record = ImportRecord::new(
            ImportId::time_ordered(),
            owner,
            new,
            self.now_to_the_second(),
        );
        let id = record.id;
        let dir = self.dir_of(id);
        fs::create_dir_all(&dir)?;
        if let Err(error) = write_manifest_atomically(&dir, &manifest_bytes(&record)) {
            delete_dir(&dir, id);
            return Err(error);
        }
        self.lock().insert(id, record);
        Ok(ImportWriter {
            store: Arc::clone(self),
            id,
        })
    }

    /// The owner's live imports, newest first.
    pub fn list(&self, owner: &UserId) -> Vec<Import> {
        let now = self.now_to_the_second();
        let imports = self.lock();
        let mut records: Vec<&ImportRecord> = imports
            .values()
            .filter(|record| record.owner == *owner && self.is_live(record, now))
            .collect();
        records.sort_by_key(|record| Reverse((record.created_at, record.id)));
        records
            .into_iter()
            .map(|record| self.render(record, now))
            .collect()
    }

    pub fn get(&self, owner: &UserId, id: ImportId) -> Option<Import> {
        let now = self.now_to_the_second();
        let imports = self.lock();
        let record = imports.get(&id)?;
        (record.owner == *owner && self.is_live(record, now)).then(|| self.render(record, now))
    }

    pub fn signed_file(
        &self,
        id: ImportId,
        name: &FileName,
        exp: i64,
        sig: &str,
    ) -> Result<PathBuf, FileError> {
        let now = self.now_to_the_second();
        if exp <= now.as_second() || !self.signer.verify(id, name, exp, sig) {
            return Err(FileError::Forbidden);
        }
        let imports = self.lock();
        let listed = imports.get(&id).is_some_and(|record| {
            self.is_live(record, now) && record.files.iter().any(|file| file.name == *name)
        });
        if listed {
            Ok(self.dir_of(id).join(name.as_str()))
        } else {
            Err(FileError::NotFound)
        }
    }

    /// Deletes expired imports. A delete that fails is retried on the next sweep.
    pub fn sweep(&self) {
        let now = self.now_to_the_second();
        let expired: Vec<ImportId> = self
            .lock()
            .values()
            .filter(|record| !self.is_live(record, now))
            .map(|record| record.id)
            .collect();
        for id in expired {
            if delete_dir(&self.dir_of(id), id) {
                self.lock().remove(&id);
                tracing::info!(%id, "deleted an expired import");
            }
        }
    }

    fn reload(&self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let now = self.now_to_the_second();
        let mut kept = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok())
            else {
                continue;
            };
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir = entry.path();
            let mut record = match read_manifest(&dir, id) {
                Ok(Manifest::Record(record)) => *record,
                Ok(Manifest::Broken(reason)) => {
                    tracing::warn!(%id, reason, "deleting an import whose manifest can't be parsed");
                    delete_dir(&dir, id);
                    continue;
                }
                Err(error) => {
                    tracing::warn!(%id, %error, "skipping an import whose manifest can't be read");
                    continue;
                }
            };
            if !self.is_live(&record, now) {
                delete_dir(&dir, id);
                continue;
            }
            if record.interrupt_if_unfinished(now)
                && let Err(error) = write_manifest_atomically(&dir, &manifest_bytes(&record))
            {
                tracing::error!(%id, %error, "cannot write the import manifest");
            }
            kept.push(record);
        }
        self.lock()
            .extend(kept.into_iter().map(|record| (record.id, record)));
        Ok(())
    }

    fn render(&self, record: &ImportRecord, now: Timestamp) -> Import {
        let expires_at = self.expires_at(record);
        let link_expiry = expires_at
            .unwrap_or_else(|| self.after_retention(now))
            .as_second();
        Import::new(record, expires_at, |name| {
            self.signer.url(record.id, name, link_expiry)
        })
    }

    fn is_live(&self, record: &ImportRecord, now: Timestamp) -> bool {
        self.expires_at(record)
            .is_none_or(|expires_at| now < expires_at)
    }

    fn expires_at(&self, record: &ImportRecord) -> Option<Timestamp> {
        record
            .progress
            .finished_at()
            .map(|finished_at| self.after_retention(finished_at))
    }

    fn after_retention(&self, time: Timestamp) -> Timestamp {
        time.checked_add(self.retention).unwrap_or(Timestamp::MAX)
    }

    fn now_to_the_second(&self) -> Timestamp {
        let now = self.clock.now();
        Timestamp::from_second(now.as_second()).unwrap_or(now)
    }

    fn dir_of(&self, id: ImportId) -> PathBuf {
        self.dir.join(id.to_string())
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<ImportId, ImportRecord>> {
        self.imports.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub async fn sweep_every(store: Arc<ImportStore>, period: Duration) {
    let mut ticks = tokio::time::interval(period);
    loop {
        ticks.tick().await;
        let store = Arc::clone(&store);
        if let Err(error) = tokio::task::spawn_blocking(move || store.sweep()).await {
            tracing::error!(%error, "the sweep panicked");
        }
    }
}

/// The one handle that may change an import. The runner gets it from `ImportStore::create`
/// and gives it up with `succeed` or `fail`. Dropping it earlier fails the import with
/// `interrupted`, so an import is never left running without a runner.
pub struct ImportWriter {
    store: Arc<ImportStore>,
    id: ImportId,
}

impl ImportWriter {
    pub fn id(&self) -> ImportId {
        self.id
    }

    /// Where the runner puts the import's files.
    pub fn dir(&self) -> PathBuf {
        self.store.dir_of(self.id)
    }

    pub fn start(&mut self) {
        self.update(|record| record.progress = Progress::Running);
    }

    pub fn set_step(&mut self, key: StepKey, state: StepState, detail: Option<StepDetail>) {
        self.update(|record| record.steps.set(key, state, detail));
    }

    pub fn set_title(&mut self, title: String) {
        self.update(|record| record.title = Some(title));
    }

    pub fn warn(&mut self, kind: WarningKind) {
        self.update(|record| {
            if !record.warnings.contains(&kind) {
                record.warnings.push(kind);
            }
        });
    }

    /// Lists a file the runner wrote to `dir()`, with its size on disk.
    pub fn add_file(&mut self, name: FileName) -> io::Result<()> {
        let bytes = fs::metadata(self.dir().join(name.as_str()))?.len();
        self.update(|record| record.add_file(name, bytes));
        Ok(())
    }

    pub fn succeed(mut self, recipe: RecipeRef) {
        let finished_at = self.store.now_to_the_second();
        self.update(|record| {
            record.progress = Progress::Succeeded {
                recipe,
                finished_at,
            };
        });
    }

    pub fn fail(mut self, error: ImportErrorKind) {
        let finished_at = self.store.now_to_the_second();
        self.update(|record| record.progress = Progress::Failed { error, finished_at });
    }

    fn update(&mut self, change: impl FnOnce(&mut ImportRecord)) {
        self.update_if(|record| {
            change(record);
            true
        });
    }

    fn update_if(&mut self, change: impl FnOnce(&mut ImportRecord) -> bool) {
        let bytes = {
            let mut imports = self.store.lock();
            let record = imports
                .get_mut(&self.id)
                .expect("the sweeper never removes an import that has a writer");
            if !change(record) {
                return;
            }
            manifest_bytes(record)
        };
        if let Err(error) = write_manifest_atomically(&self.dir(), &bytes) {
            tracing::error!(id = %self.id, %error, "cannot write the import manifest");
        }
    }
}

impl Drop for ImportWriter {
    fn drop(&mut self) {
        let now = self.store.now_to_the_second();
        self.update_if(|record| {
            let interrupted = record.interrupt_if_unfinished(now);
            if interrupted {
                tracing::warn!(id = %record.id, "an import stopped before it finished");
            }
            interrupted
        });
    }
}

enum Manifest {
    Record(Box<ImportRecord>),
    Broken(String),
}

fn read_manifest(dir: &Path, id: ImportId) -> io::Result<Manifest> {
    let bytes = match fs::read(dir.join(MANIFEST)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Manifest::Broken("there is no manifest".to_owned()));
        }
        Err(error) => return Err(error),
    };
    Ok(match serde_json::from_slice::<ImportRecord>(&bytes) {
        Ok(record) if record.id == id => Manifest::Record(Box::new(record)),
        Ok(_) => Manifest::Broken("the id doesn't match the directory".to_owned()),
        Err(error) => Manifest::Broken(error.to_string()),
    })
}

fn manifest_bytes(record: &ImportRecord) -> Vec<u8> {
    serde_json::to_vec_pretty(record).expect("a record always serializes")
}

fn write_manifest_atomically(dir: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = dir.join(MANIFEST_TMP);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, dir.join(MANIFEST))
}

fn delete_dir(dir: &Path, id: ImportId) -> bool {
    match fs::remove_dir_all(dir) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => {
            tracing::warn!(%id, %error, "cannot delete the import directory");
            false
        }
    }
}
