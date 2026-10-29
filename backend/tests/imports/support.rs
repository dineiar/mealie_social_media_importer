use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use mealie_importer::config::Config;
use mealie_importer::store::{
    Clock, FileName, Import, ImportId, ImportStore, ImportWriter, NewImport, RecipeRef, UserId,
};
use serde_json::Value;
use tempfile::TempDir;

pub const RETENTION: Duration = Duration::from_mins(60);
pub const START: &str = "2026-10-20T14:02:11Z";

pub struct ManualClock(Mutex<Timestamp>);

impl ManualClock {
    pub fn advance(&self, by: Duration) {
        let mut now = self.0.lock().unwrap();
        *now += SignedDuration::try_from(by).unwrap();
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }
}

pub struct Harness {
    pub data_dir: TempDir,
    pub clock: Arc<ManualClock>,
    pub store: Arc<ImportStore>,
}

impl Harness {
    pub fn new() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(ManualClock(Mutex::new(START.parse().unwrap())));
        let store = ImportStore::open(data_dir.path(), RETENTION, clock.clone()).unwrap();
        Self {
            data_dir,
            clock,
            store,
        }
    }

    pub fn restart(&mut self) {
        self.store = ImportStore::open(self.data_dir.path(), RETENTION, self.clock.clone())
            .expect("the store reopens");
    }

    pub fn create(&self, owner: &UserId, url: &str) -> ImportWriter {
        let new = NewImport {
            url: url.to_owned(),
            platform: "instagram".to_owned(),
            language: Some("pt-BR".to_owned()),
        };
        self.store.create(owner.clone(), new).unwrap()
    }

    pub fn imports_dir(&self) -> PathBuf {
        self.data_dir.path().join("imports")
    }

    pub fn dir_of(&self, id: ImportId) -> PathBuf {
        self.imports_dir().join(id.to_string())
    }

    pub fn list(&self, owner: &UserId) -> Vec<Value> {
        self.store.list(owner).iter().map(json).collect()
    }

    pub fn only(&self, owner: &UserId) -> Value {
        let mut imports = self.list(owner);
        assert_eq!(imports.len(), 1, "{imports:#?}");
        imports.remove(0)
    }

    pub fn config(&self) -> Config {
        let vars = HashMap::from([
            ("MEALIE_URL".to_owned(), "http://mealie:9000".to_owned()),
            (
                "DATA_DIR".to_owned(),
                self.data_dir.path().to_str().unwrap().to_owned(),
            ),
        ]);
        Config::from_vars(&vars).unwrap()
    }
}

pub fn alice() -> UserId {
    UserId::from("alice-id".to_owned())
}

pub fn bob() -> UserId {
    UserId::from("bob-id".to_owned())
}

pub fn json(import: &Import) -> Value {
    serde_json::to_value(import).unwrap()
}

pub fn at(time: &str) -> Value {
    Value::String(time.to_owned())
}

pub fn unix(time: &str) -> i64 {
    time.parse::<Timestamp>().unwrap().as_second()
}

pub fn exp_and_sig(link: &str) -> (String, String) {
    let (_, query) = link.split_once('?').unwrap();
    let (exp, sig) = query
        .strip_prefix("exp=")
        .and_then(|rest| rest.split_once("&sig="))
        .unwrap();
    (exp.to_owned(), sig.to_owned())
}

pub fn add_file(writer: &mut ImportWriter, name: &str, contents: &[u8]) {
    fs::write(writer.dir().join(name), contents).unwrap();
    writer.add_file(name.parse::<FileName>().unwrap()).unwrap();
}

pub fn recipe() -> RecipeRef {
    let vars = HashMap::from([(
        "MEALIE_URL".to_owned(),
        "https://recipes.example.com".to_owned(),
    )]);
    let config = Config::from_vars(&vars).unwrap();
    RecipeRef::new(
        &config.mealie_public_url,
        "home",
        &"one-pan-lemon-orzo".parse().unwrap(),
        "One-pan lemon orzo".to_owned(),
    )
}

pub type Logs = Arc<Mutex<Vec<u8>>>;

struct LogWriter(Logs);

impl io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Global, because a per-thread subscriber loses events to tracing's callsite cache when
/// another test thread reaches a callsite first.
pub fn capture_logs() -> Logs {
    static LOGS: OnceLock<Logs> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Logs::default();
        let writer = logs.clone();
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || LogWriter(writer.clone()))
            .try_init()
            .expect("no other global subscriber");
        logs
    })
    .clone()
}
