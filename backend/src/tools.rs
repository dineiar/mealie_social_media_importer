use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{self, Path, PathBuf};
use std::time::Duration;

use crate::process::{Exit, RunError, run};

pub const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

const VERSION_LOG_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tool {
    GalleryDl,
    YtDlp,
    Ffmpeg,
    Ffprobe,
}

impl Tool {
    const ALL: [Self; 4] = [Self::GalleryDl, Self::YtDlp, Self::Ffmpeg, Self::Ffprobe];

    const fn executable(self) -> &'static str {
        match self {
            Self::GalleryDl => "gallery-dl",
            Self::YtDlp => "yt-dlp",
            Self::Ffmpeg => "ffmpeg",
            Self::Ffprobe => "ffprobe",
        }
    }

    const fn version_flag(self) -> &'static str {
        match self {
            Self::GalleryDl | Self::YtDlp => "--version",
            Self::Ffmpeg | Self::Ffprobe => "-version",
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.executable())
    }
}

/// Where each tool was found. A tool with no entry is missing, and whatever depends on it
/// reports itself as not configured.
#[derive(Debug)]
pub struct ToolSet {
    found: BTreeMap<Tool, PathBuf>,
}

impl ToolSet {
    pub fn from_paths(found: impl IntoIterator<Item = (Tool, PathBuf)>) -> Self {
        Self {
            found: found.into_iter().collect(),
        }
    }

    /// Looks in the virtualenv at `<data_dir>/tools/bin` first, then in each directory of
    /// `path_var`. A relative directory is skipped.
    pub fn discover(data_dir: &Path, path_var: Option<&OsStr>) -> Self {
        let venv_bin = path::absolute(data_dir.join("tools").join("bin")).ok();
        let path_dirs = path_var
            .into_iter()
            .flat_map(env::split_paths)
            .filter(|dir| dir.is_absolute());
        let dirs: Vec<PathBuf> = venv_bin.into_iter().chain(path_dirs).collect();

        Self::from_paths(Tool::ALL.into_iter().filter_map(|tool| {
            dirs.iter()
                .map(|dir| dir.join(tool.executable()))
                .find(|candidate| is_executable(candidate))
                .map(|path| (tool, path))
        }))
    }

    pub fn path(&self, tool: Tool) -> Option<&Path> {
        self.found.get(&tool).map(PathBuf::as_path)
    }

    /// A version check that outlasts `timeout` is killed.
    pub async fn log_versions(&self, timeout: Duration) {
        for tool in Tool::ALL {
            let Some(path) = self.path(tool) else {
                tracing::warn!(%tool, "tool not found in the tools virtualenv or on PATH");
                continue;
            };
            match version(tool, path, timeout).await {
                Ok(version) => tracing::info!(%tool, path = %path.display(), version, "tool found"),
                Err(problem) => tracing::warn!(
                    %tool,
                    path = %path.display(),
                    %problem,
                    "tool found, but its version is unknown"
                ),
            }
        }
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[derive(Debug, thiserror::Error)]
enum VersionError {
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("version check ended with {0}")]
    Exit(Exit),
    #[error("version check printed nothing")]
    Silent,
}

async fn version(tool: Tool, path: &Path, timeout: Duration) -> Result<String, VersionError> {
    let output = run(path, [tool.version_flag()], None, timeout).await?;
    if !output.exit.success() {
        return Err(VersionError::Exit(output.exit));
    }
    output
        .stdout
        .text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(VERSION_LOG_CHARS).collect())
        .ok_or(VersionError::Silent)
}
