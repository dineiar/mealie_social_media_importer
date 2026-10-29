use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process_group};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

use crate::tools::ToolHandle;

/// Each of stdout and stderr keeps this many bytes. The rest is read and dropped.
pub const OUTPUT_CAP: usize = 1024 * 1024;

/// How long to keep reading the pipes once the child has exited or been killed. A process
/// the child left behind can hold a pipe open forever. It is killed when it still holds one
/// after this long.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

const CHILD_ENV: &[&str] = &[
    // yt-dlp finds ffmpeg and ffprobe through it.
    "PATH",
    // gallery-dl and yt-dlp look for their config and cache under it.
    "HOME",
    // Where ffmpeg and yt-dlp write scratch files.
    "TMPDIR",
    // Time zone of dates in file names and metadata.
    "TZ",
    // Text encoding of file names and tool output.
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    // Downloads go through the proxy the operator configured.
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    // The CA bundle or directory the operator trusts for HTTPS downloads. REQUESTS_CA_BUNDLE
    // is the one Python's requests reads.
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "REQUESTS_CA_BUNDLE",
];

/// An empty or relative `PATH` entry means the child's working directory, which is the
/// import directory holding downloaded files. Only absolute entries are passed on.
fn child_env() -> impl Iterator<Item = (&'static str, OsString)> {
    CHILD_ENV.iter().filter_map(|&name| {
        let value = env::var_os(name)?;
        if name != "PATH" {
            return Some((name, value));
        }
        let joined =
            env::join_paths(env::split_paths(&value).filter(|dir| dir.is_absolute())).ok()?;
        (!joined.is_empty()).then_some((name, joined))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
    TimedOut,
}

impl Exit {
    pub fn success(self) -> bool {
        self == Self::Code(0)
    }

    fn from_status(status: ExitStatus) -> Self {
        status
            .code()
            .map(Self::Code)
            .or(status.signal().map(Self::Signal))
            .expect("a waited-for child either exited or was killed by a signal")
    }
}

impl fmt::Display for Exit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(code) => write!(f, "exit code {code}"),
            Self::Signal(signal) => write!(f, "signal {signal}"),
            Self::TimedOut => f.write_str("timed out"),
        }
    }
}

#[derive(Debug)]
pub struct Captured {
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug)]
pub struct RunOutput {
    pub exit: Exit,
    pub stdout: Captured,
    pub stderr: Captured,
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("the working directory {} does not exist", dir.display())]
    MissingWorkingDirectory { dir: PathBuf },
    #[error("cannot start {}: {source}", program.display())]
    Spawn { program: PathBuf, source: io::Error },
    #[error("cannot read the output of {}: {source}", program.display())]
    Read { program: PathBuf, source: io::Error },
}

/// Runs `tool` with `args` as its argument vector, never through a shell, and waits for it for
/// at most `timeout`. On a timeout, or when the returned future is dropped, the child and
/// everything it started are killed.
///
/// A path is not a tool, so a caller cannot name a program of its own:
///
/// ```compile_fail,E0308
/// use std::path::Path;
/// use std::time::Duration;
/// use mealie_importer::process::run;
///
/// async fn run_any_program() {
///     let _ = run(Path::new("/usr/bin/id"), ["-u"], None, Duration::from_secs(1)).await;
/// }
/// ```
pub async fn run<I, S>(
    tool: &ToolHandle,
    args: I,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<RunOutput, RunError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    #[allow(
        clippy::disallowed_methods,
        reason = "the one place a process is spawned"
    )]
    let mut command = Command::new(tool.path());
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(child_env())
        .kill_on_drop(true)
        .process_group(0);
    if let Some(cwd) = cwd {
        // A failed spawn reports NotFound for a missing directory as well as a missing
        // program, so the directory is checked first to keep the two apart.
        if !cwd.is_dir() {
            return Err(RunError::MissingWorkingDirectory {
                dir: cwd.to_owned(),
            });
        }
        command.current_dir(cwd);
    }
    let program = || tool.path().to_owned();
    let child = command.spawn().map_err(|source| RunError::Spawn {
        program: program(),
        source,
    })?;
    supervise(child, timeout)
        .await
        .map_err(|source| RunError::Read {
            program: program(),
            source,
        })
}

async fn supervise(mut child: Child, timeout: Duration) -> io::Result<RunOutput> {
    let mut group = GroupKill::of(&child);
    let mut stdout = Capture::new(child.stdout.take().expect("stdout is piped"));
    let mut stderr = Capture::new(child.stderr.take().expect("stderr is piped"));

    let waited = tokio::time::timeout(timeout, async {
        tokio::select! {
            status = child.wait() => status,
            read = fill_both(&mut stdout, &mut stderr) => {
                read?;
                child.wait().await
            }
        }
    })
    .await;
    let exit = match waited {
        Ok(status) => Exit::from_status(status?),
        Err(_elapsed) => {
            group.kill();
            let _ = tokio::time::timeout(DRAIN_GRACE, child.wait()).await;
            Exit::TimedOut
        }
    };
    if tokio::time::timeout(DRAIN_GRACE, fill_both(&mut stdout, &mut stderr))
        .await
        .is_ok()
    {
        group.disarm();
    }
    Ok(RunOutput {
        exit,
        stdout: stdout.finish(),
        stderr: stderr.finish(),
    })
}

async fn fill_both(
    stdout: &mut Capture<ChildStdout>,
    stderr: &mut Capture<ChildStderr>,
) -> io::Result<()> {
    tokio::try_join!(stdout.fill(), stderr.fill()).map(|_| ())
}

struct Capture<R> {
    pipe: R,
    bytes: Vec<u8>,
    truncated: bool,
}

impl<R: AsyncRead + Unpin> Capture<R> {
    fn new(pipe: R) -> Self {
        Self {
            pipe,
            bytes: Vec::new(),
            truncated: false,
        }
    }

    async fn fill(&mut self) -> io::Result<()> {
        let mut chunk = [0u8; 8 * 1024];
        loop {
            let read = self.pipe.read(&mut chunk).await?;
            if read == 0 {
                return Ok(());
            }
            let keep = read.min(OUTPUT_CAP - self.bytes.len());
            self.bytes.extend_from_slice(&chunk[..keep]);
            self.truncated |= keep < read;
        }
    }

    fn finish(self) -> Captured {
        let text = String::from_utf8(self.bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
        Captured {
            text,
            truncated: self.truncated,
        }
    }
}

/// `kill_on_drop` reaches only the child, and a tool such as yt-dlp starts helpers of its
/// own. The guard stays armed until both pipes close, which shows that no helper still holds
/// them. The kernel keeps a group id reserved while any member lives, so a kill after the
/// child is reaped can only reach that group.
struct GroupKill(Option<Pid>);

impl GroupKill {
    fn of(child: &Child) -> Self {
        Self(
            child
                .id()
                .and_then(|id| i32::try_from(id).ok())
                .and_then(Pid::from_raw),
        )
    }

    fn disarm(&mut self) {
        self.0 = None;
    }

    fn kill(&mut self) {
        if let Some(group) = self.0.take() {
            let _ = kill_process_group(group, Signal::KILL);
        }
    }
}

impl Drop for GroupKill {
    fn drop(&mut self) {
        self.kill();
    }
}
