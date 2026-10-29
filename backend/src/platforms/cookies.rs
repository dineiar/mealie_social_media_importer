use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use sha2::{Digest, Sha256};

use super::{PlatformStatus, SetupHint};

#[derive(Debug, Clone, Copy)]
pub struct SessionCookie {
    pub domain: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckIn {
    Saved,
    /// The original was replaced after the checkout. Its cookies win.
    Stale,
    /// The job's copy holds no session, so keeping it would lose the one the store has.
    Unusable,
}

#[derive(Debug, thiserror::Error)]
pub enum CookieError {
    #[error("no cookies file is configured")]
    NoOriginal,
    #[error("cannot {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

/// A job's private copy of the working cookies. A job that fails drops it, so nothing
/// returns to the store. Only a successful job passes it to [`CookieStore::check_in`].
#[derive(Debug)]
pub struct Checkout {
    path: PathBuf,
    seeded_from: String,
}

impl Checkout {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone)]
pub struct CookieStore {
    inner: Arc<Inner>,
}

struct Inner {
    original: PathBuf,
    working: PathBuf,
    source: PathBuf,
    session: SessionCookie,
    lock: Mutex<()>,
}

impl CookieStore {
    pub fn new(dir: &Path, platform: &str, original: PathBuf, session: SessionCookie) -> Self {
        Self {
            inner: Arc::new(Inner {
                original,
                working: dir.join(format!("{platform}.txt")),
                source: dir.join(format!("{platform}.source")),
                session,
                lock: Mutex::new(()),
            }),
        }
    }

    pub async fn status(&self) -> Result<PlatformStatus, CookieError> {
        self.blocking(Inner::status).await
    }

    pub async fn check_out(&self, copy: &Path) -> Result<Checkout, CookieError> {
        let copy = copy.to_owned();
        self.blocking(move |inner| inner.check_out(copy)).await
    }

    pub async fn check_in(&self, checkout: Checkout) -> Result<CheckIn, CookieError> {
        self.blocking(move |inner| inner.check_in(&checkout)).await
    }

    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Inner) -> T + Send + 'static,
    ) -> T {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || work(&inner))
            .await
            .expect("the cookie task doesn't panic")
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn status(&self) -> Result<PlatformStatus, CookieError> {
        let _guard = self.lock();
        if self.seed()?.is_none() {
            return Ok(PlatformStatus::NotConfigured(SetupHint::CookiesMissing));
        }
        Ok(assess(&read(&self.working)?, self.session))
    }

    fn check_out(&self, copy: PathBuf) -> Result<Checkout, CookieError> {
        let _guard = self.lock();
        let seeded_from = self.seed()?.ok_or(CookieError::NoOriginal)?;
        let working = read(&self.working)?;
        replace_file(&copy, &working)?;
        Ok(Checkout {
            path: copy,
            seeded_from,
        })
    }

    fn check_in(&self, checkout: &Checkout) -> Result<CheckIn, CookieError> {
        let _guard = self.lock();
        if self.recorded_source().as_deref() != Some(checkout.seeded_from.as_str()) {
            return Ok(CheckIn::Stale);
        }
        let copy = read(&checkout.path)?;
        if assess(&copy, self.session) != PlatformStatus::Ready {
            return Ok(CheckIn::Unusable);
        }
        replace_file(&self.working, &copy)?;
        Ok(CheckIn::Saved)
    }

    fn seed(&self) -> Result<Option<String>, CookieError> {
        let original = match fs::read(&self.original) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("read", &self.original, error)),
        };
        let fingerprint = fingerprint(&original);
        let seeded = self.recorded_source().as_deref() == Some(fingerprint.as_str())
            && self.working.is_file();
        if !seeded {
            if let Some(dir) = self.working.parent() {
                fs::create_dir_all(dir).map_err(|error| io_error("create", dir, error))?;
            }
            replace_file(&self.working, &original)?;
            replace_file(&self.source, fingerprint.as_bytes())?;
        }
        Ok(Some(fingerprint))
    }

    fn recorded_source(&self) -> Option<String> {
        let text = fs::read_to_string(&self.source).ok()?;
        Some(text.trim().to_owned())
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read(path: &Path) -> Result<Vec<u8>, CookieError> {
    fs::read(path).map_err(|error| io_error("read", path, error))
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> CookieError {
    CookieError::Io {
        action,
        path: path.to_owned(),
        source,
    }
}

fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), CookieError> {
    let temp = path.with_added_extension("tmp");
    write_private(&temp, bytes)
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|error| io_error("write", path, error))
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Reads a Netscape `cookies.txt` the way gallery-dl does: `#HttpOnly_` lines are cookies,
/// other `#` lines and blank lines are skipped, and every other line must have seven
/// tab-separated fields. The tool matches the domain exactly, and so does this.
fn assess(bytes: &[u8], session: SessionCookie) -> PlatformStatus {
    let invalid = PlatformStatus::NotConfigured(SetupHint::CookiesInvalid);
    let Ok(text) = std::str::from_utf8(bytes) else {
        return invalid;
    };
    let mut found = false;
    for line in text.lines() {
        let line = line.trim_start_matches(' ');
        let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if line.is_empty() || line.starts_with(['#', '$']) {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let [domain, _, _, _, _, name, value] = fields[..] else {
            return invalid;
        };
        found |= domain == session.domain && name == session.name && !value.is_empty();
    }
    if found {
        PlatformStatus::Ready
    } else {
        PlatformStatus::NotConfigured(SetupHint::CookiesNoSession)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use PlatformStatus::{NotConfigured, Ready};
    use SetupHint::{CookiesInvalid, CookiesMissing, CookiesNoSession};

    const SESSION: SessionCookie = SessionCookie {
        domain: ".instagram.com",
        name: "sessionid",
    };

    const HEADER: &str = "# Netscape HTTP Cookie File\n";

    fn line(domain: &str, name: &str, value: &str) -> String {
        format!("{domain}\tTRUE\t/\tTRUE\t1800000000\t{name}\t{value}\n")
    }

    fn jar(cookies: &[(&str, &str)]) -> String {
        let lines: String = cookies
            .iter()
            .map(|(name, value)| line(".instagram.com", name, value))
            .collect();
        format!("{HEADER}{lines}")
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn text(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    struct Fixture {
        dir: TempDir,
        jobs: Cell<u32>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                dir: TempDir::new().unwrap(),
                jobs: Cell::new(0),
            }
        }

        fn original(&self) -> PathBuf {
            self.dir.path().join("config/instagram.txt")
        }

        fn cookies_dir(&self) -> PathBuf {
            self.dir.path().join("data/cookies")
        }

        fn working(&self) -> PathBuf {
            self.cookies_dir().join("instagram.txt")
        }

        fn source(&self) -> PathBuf {
            self.cookies_dir().join("instagram.source")
        }

        fn store(&self) -> CookieStore {
            CookieStore::new(&self.cookies_dir(), "instagram", self.original(), SESSION)
        }

        fn write_original(&self, contents: impl AsRef<[u8]>) {
            fs::create_dir_all(self.original().parent().unwrap()).unwrap();
            fs::write(self.original(), contents).unwrap();
        }

        async fn check_out(&self, store: &CookieStore) -> Checkout {
            let job = self.jobs.get();
            self.jobs.set(job + 1);
            let copy = self.dir.path().join(format!("job-{job}.txt"));
            store.check_out(&copy).await.unwrap()
        }

        async fn finish_job(
            &self,
            store: &CookieStore,
            checkout: Checkout,
            rewritten: &str,
        ) -> CheckIn {
            fs::write(checkout.path(), rewritten).unwrap();
            store.check_in(checkout).await.unwrap()
        }
    }

    async fn status_of(contents: impl AsRef<[u8]>) -> PlatformStatus {
        let fixture = Fixture::new();
        fixture.write_original(contents);
        fixture.store().status().await.unwrap()
    }

    #[tokio::test]
    async fn without_an_original_the_store_is_not_configured_and_writes_nothing() {
        let fixture = Fixture::new();
        let store = fixture.store();

        assert_eq!(store.status().await.unwrap(), NotConfigured(CookiesMissing));
        let copy = fixture.dir.path().join("job.txt");
        assert!(matches!(
            store.check_out(&copy).await,
            Err(CookieError::NoOriginal)
        ));
        assert!(!fixture.cookies_dir().exists());
    }

    #[tokio::test]
    async fn status_turns_ready_when_the_original_appears_without_a_new_store() {
        let fixture = Fixture::new();
        let store = fixture.store();
        assert_eq!(store.status().await.unwrap(), NotConfigured(CookiesMissing));

        fixture.write_original(jar(&[("sessionid", "s1")]));

        assert_eq!(store.status().await.unwrap(), Ready);
    }

    #[tokio::test]
    async fn no_original_means_not_configured_whatever_the_working_copy_holds() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        let store = fixture.store();
        assert_eq!(store.status().await.unwrap(), Ready);

        fs::remove_file(fixture.original()).unwrap();

        assert_eq!(store.status().await.unwrap(), NotConfigured(CookiesMissing));
        assert!(fixture.working().exists());
    }

    #[tokio::test]
    async fn an_unreadable_original_is_an_error_that_names_the_path() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.original()).unwrap();

        let error = fixture.store().status().await.unwrap_err();

        assert!(matches!(error, CookieError::Io { .. }));
        assert!(
            error.to_string().contains("config/instagram.txt"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_cookie_file_is_ready_only_with_a_session_for_the_exact_domain() {
        let session = |domain: &str| format!("{HEADER}{}", line(domain, "sessionid", "s1"));
        let ready = [
            jar(&[("sessionid", "s1")]),
            jar(&[("mid", "m"), ("sessionid", "s1"), ("rur", "r")]),
            format!(
                "{HEADER}#HttpOnly_{}",
                line(".instagram.com", "sessionid", "s1")
            ),
            format!(
                "{HEADER}\n# a comment\n$ another\n{}\n",
                line(".instagram.com", "sessionid", "s1")
            ),
            format!(
                "# HTTP Cookie File\r\n{}",
                line(".instagram.com", "sessionid", "s1").replace('\n', "\r\n")
            ),
            format!("{HEADER}.instagram.com\tTRUE\t/\tTRUE\t0\tsessionid\ts1\n"),
            format!("{HEADER}  {}", line(".instagram.com", "sessionid", "s1")),
        ];
        let no_session = [
            String::new(),
            HEADER.to_owned(),
            jar(&[("sessionid", "")]),
            jar(&[("mid", "m"), ("rur", "r")]),
            jar(&[("sessionid2", "s1")]),
            session("www.instagram.com"),
            session("instagram.com"),
            session(".example.com"),
            session(".instagram.com.evil.test"),
        ];
        let invalid = [
            "<html><body>Log in</body></html>\n".to_owned(),
            "[{\"name\": \"sessionid\", \"value\": \"s1\"}]\n".to_owned(),
            format!("{HEADER}.instagram.com\tTRUE\t/\tTRUE\t1800000000\tsessionid\n"),
            format!("{HEADER}.instagram.com\tTRUE\t/\tTRUE\t1800000000\tsessionid\ts1\textra\n"),
            format!("{HEADER}.instagram.com TRUE / TRUE 1800000000 sessionid s1\n"),
            format!("{}not a cookie\n", jar(&[("sessionid", "s1")])),
        ];
        for (contents, expected) in [
            (&ready[..], Ready),
            (&no_session[..], NotConfigured(CookiesNoSession)),
            (&invalid[..], NotConfigured(CookiesInvalid)),
        ] {
            for contents in contents {
                assert_eq!(status_of(contents).await, expected, "{contents:?}");
            }
        }
        assert_eq!(
            status_of([0xff, 0xfe, 0x00]).await,
            NotConfigured(CookiesInvalid)
        );
    }

    #[tokio::test]
    async fn the_working_copy_is_seeded_from_the_original_and_records_its_sha256() {
        let fixture = Fixture::new();
        let original = jar(&[("sessionid", "s1")]);
        fixture.write_original(&original);

        fixture.store().status().await.unwrap();

        assert_eq!(text(&fixture.working()), original);
        assert_eq!(
            text(&fixture.source()).trim(),
            "a42c2883b55486e6b09093533327bcd136d82b9267647e0738157efe754982d3"
        );
    }

    #[tokio::test]
    async fn the_original_is_never_written() {
        let fixture = Fixture::new();
        let original = jar(&[("sessionid", "s1")]);
        fixture.write_original(&original);
        fs::set_permissions(fixture.original(), Permissions::from_mode(0o444)).unwrap();
        let store = fixture.store();

        let job = fixture.check_out(&store).await;
        fixture
            .finish_job(&store, job, &jar(&[("sessionid", "s1"), ("rur", "r")]))
            .await;

        assert_eq!(text(&fixture.original()), original);
        assert_eq!(mode(&fixture.original()), 0o444);
    }

    #[tokio::test]
    async fn refreshed_cookies_survive_a_restart() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1"), ("mid", "m1")]));
        let store = fixture.store();
        let refreshed = jar(&[("sessionid", "s1"), ("mid", "m1"), ("rur", "refreshed")]);
        let job = fixture.check_out(&store).await;
        assert_eq!(
            fixture.finish_job(&store, job, &refreshed).await,
            CheckIn::Saved
        );
        drop(store);

        let restarted = fixture.store();
        let next = fixture.check_out(&restarted).await;

        assert_eq!(text(next.path()), refreshed);
        assert_eq!(restarted.status().await.unwrap(), Ready);
    }

    #[tokio::test]
    async fn replacing_the_original_reseeds_the_working_copy() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        let store = fixture.store();
        let job = fixture.check_out(&store).await;
        fixture
            .finish_job(&store, job, &jar(&[("sessionid", "s1"), ("rur", "r")]))
            .await;
        let replacement = jar(&[("sessionid", "s2")]);

        fixture.write_original(&replacement);
        let next = fixture.check_out(&fixture.store()).await;

        assert_eq!(text(next.path()), replacement);
        assert_eq!(text(&fixture.working()), replacement);
    }

    #[tokio::test]
    async fn a_failed_run_checks_nothing_in() {
        let fixture = Fixture::new();
        let original = jar(&[("sessionid", "s1"), ("mid", "m1")]);
        fixture.write_original(&original);
        let store = fixture.store();

        let failed = fixture.check_out(&store).await;
        fs::write(failed.path(), jar(&[("mid", "m1")])).unwrap();
        drop(failed);

        assert_eq!(store.status().await.unwrap(), Ready);
        let next = fixture.check_out(&store).await;
        assert_eq!(text(next.path()), original);
    }

    #[tokio::test]
    async fn a_copy_that_lost_its_session_is_not_checked_in() {
        let fixture = Fixture::new();
        let original = jar(&[("sessionid", "s1")]);
        fixture.write_original(&original);
        let store = fixture.store();

        for lost in [
            jar(&[("mid", "m1")]),
            jar(&[("sessionid", "")]),
            "not a cookie file".to_owned(),
        ] {
            let job = fixture.check_out(&store).await;

            let outcome = fixture.finish_job(&store, job, &lost).await;

            assert_eq!(outcome, CheckIn::Unusable, "{lost:?}");
            assert_eq!(text(&fixture.working()), original);
        }
    }

    #[tokio::test]
    async fn a_check_in_from_before_the_original_was_replaced_is_dropped() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        let store = fixture.store();
        let job = fixture.check_out(&store).await;
        let replacement = jar(&[("sessionid", "s2")]);
        fixture.write_original(&replacement);
        store.status().await.unwrap();

        let late = jar(&[("sessionid", "s1"), ("rur", "late")]);

        assert_eq!(fixture.finish_job(&store, job, &late).await, CheckIn::Stale);
        assert_eq!(text(&fixture.working()), replacement);
    }

    #[tokio::test]
    async fn a_replacement_nobody_has_looked_at_yet_still_wins_over_a_check_in() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        let store = fixture.store();
        let job = fixture.check_out(&store).await;
        let replacement = jar(&[("sessionid", "s2")]);
        fixture.write_original(&replacement);

        let late = jar(&[("sessionid", "s1"), ("rur", "late")]);
        fixture.finish_job(&store, job, &late).await;
        let next = fixture.check_out(&store).await;

        assert_eq!(text(next.path()), replacement);
    }

    #[tokio::test]
    async fn every_file_the_store_writes_ends_at_mode_0600() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        fs::set_permissions(fixture.original(), Permissions::from_mode(0o644)).unwrap();
        let store = fixture.store();

        let job = fixture.check_out(&store).await;
        assert_eq!(mode(&fixture.working()), 0o600);
        assert_eq!(mode(&fixture.source()), 0o600);
        assert_eq!(mode(job.path()), 0o600);

        let refreshed = jar(&[("sessionid", "s1"), ("rur", "refreshed")]);
        fs::set_permissions(job.path(), Permissions::from_mode(0o644)).unwrap();
        fixture.finish_job(&store, job, &refreshed).await;

        assert_eq!(mode(&fixture.working()), 0o600);
    }

    #[tokio::test]
    async fn a_retry_after_a_tool_died_mid_write_still_gets_a_private_copy() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        let copy = fixture.dir.path().join("job.txt");
        let leftover = fixture.dir.path().join("job.txt.tmp");
        fs::write(&leftover, "half a file").unwrap();
        fs::set_permissions(&leftover, Permissions::from_mode(0o644)).unwrap();

        let checkout = fixture.store().check_out(&copy).await.unwrap();

        assert_eq!(mode(checkout.path()), 0o600);
    }

    #[tokio::test]
    async fn a_missing_working_copy_is_seeded_again() {
        let fixture = Fixture::new();
        let original = jar(&[("sessionid", "s1")]);
        fixture.write_original(&original);
        let store = fixture.store();
        store.status().await.unwrap();
        fs::remove_file(fixture.working()).unwrap();

        assert_eq!(store.status().await.unwrap(), Ready);

        assert_eq!(text(&fixture.working()), original);
    }

    #[tokio::test]
    async fn a_reseed_that_fails_halfway_is_repeated_not_skipped() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        let store = fixture.store();
        store.status().await.unwrap();
        let replacement = jar(&[("sessionid", "s2")]);
        fixture.write_original(&replacement);
        let blocker = fixture.cookies_dir().join("instagram.txt.tmp");
        fs::create_dir(&blocker).unwrap();

        assert!(store.status().await.is_err());
        fs::remove_dir(&blocker).unwrap();

        assert_eq!(store.status().await.unwrap(), Ready);
        assert_eq!(text(&fixture.working()), replacement);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_check_ins_leave_exactly_one_whole_copy() {
        let fixture = Fixture::new();
        fixture.write_original(jar(&[("sessionid", "s1")]));
        let store = fixture.store();
        let mut checkouts = Vec::new();
        let mut copies = Vec::new();
        for job in 0..8 {
            let checkout = fixture.check_out(&store).await;
            let copy = jar(&[("sessionid", &job.to_string().repeat(200_000))]);
            fs::write(checkout.path(), &copy).unwrap();
            checkouts.push(checkout);
            copies.push(copy);
        }

        let tasks: Vec<_> = checkouts
            .into_iter()
            .map(|checkout| {
                let store = store.clone();
                tokio::spawn(async move { store.check_in(checkout).await })
            })
            .collect();
        for task in tasks {
            assert_eq!(task.await.unwrap().unwrap(), CheckIn::Saved);
        }

        assert!(
            copies.contains(&text(&fixture.working())),
            "a torn working copy"
        );
    }
}
