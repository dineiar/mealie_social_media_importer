use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::process::{Exit, RunError, RunOutput, run};
use crate::tools::{Tool, ToolHandle, ToolSet};

const STDERR_TAIL_CHARS: usize = 400;

const FFPROBE_ARGS: [&str; 8] = [
    "-v",
    "error",
    "-select_streams",
    "a:0",
    "-show_entries",
    "stream=codec_type:format=duration",
    "-of",
    "json",
];

const FFMPEG_BEFORE_INPUT: [&str; 5] = ["-nostdin", "-loglevel", "error", "-y", "-i"];

/// 32 kbps mono is about 0.24 MB per minute and enough for speech.
const FFMPEG_AFTER_INPUT: [&str; 8] = [
    "-map",
    "0:a:0",
    "-ac",
    "1",
    "-codec:a",
    "libmp3lame",
    "-b:a",
    "32k",
];

#[derive(Debug, Clone)]
pub struct AudioTools {
    ffmpeg: ToolHandle,
    ffprobe: ToolHandle,
    timeout: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("cannot run {tool}")]
    Run {
        tool: Tool,
        #[source]
        source: RunError,
    },
    #[error("{tool} ended with {exit}")]
    Failed {
        tool: Tool,
        exit: Exit,
        stderr: String,
    },
    #[error("ffprobe printed no usable duration")]
    UnusableProbe,
}

impl AudioTools {
    pub fn from_tool_set(tools: &ToolSet, timeout: Duration) -> Result<Self, Tool> {
        let find = |tool| tools.get(tool).cloned().ok_or(tool);
        Ok(Self {
            ffmpeg: find(Tool::Ffmpeg)?,
            ffprobe: find(Tool::Ffprobe)?,
            timeout,
        })
    }

    /// Replaces any file at `dest`. Returns the length ffprobe reports for the video, or `None`
    /// when the video has no audio track and nothing was written.
    pub async fn extract(&self, video: &Path, dest: &Path) -> Result<Option<Duration>, AudioError> {
        let Some(duration) = self.probe(video).await? else {
            return Ok(None);
        };
        let args = FFMPEG_BEFORE_INPUT
            .map(OsStr::new)
            .into_iter()
            .chain([video.as_os_str()])
            .chain(FFMPEG_AFTER_INPUT.map(OsStr::new))
            .chain([dest.as_os_str()]);
        self.run(Tool::Ffmpeg, &self.ffmpeg, args).await?;
        Ok(Some(duration))
    }

    async fn probe(&self, video: &Path) -> Result<Option<Duration>, AudioError> {
        let args = FFPROBE_ARGS
            .map(OsStr::new)
            .into_iter()
            .chain([OsStr::new("-i"), video.as_os_str()]);
        let output = self.run(Tool::Ffprobe, &self.ffprobe, args).await?;
        let report: Report =
            serde_json::from_str(&output.stdout.text).map_err(|_| AudioError::UnusableProbe)?;
        if report.streams.is_empty() {
            return Ok(None);
        }
        report
            .format
            .duration
            .as_deref()
            .and_then(|seconds| seconds.parse::<f64>().ok())
            .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
            .map(Some)
            .ok_or(AudioError::UnusableProbe)
    }

    async fn run<'a>(
        &self,
        tool: Tool,
        handle: &ToolHandle,
        args: impl IntoIterator<Item = &'a OsStr>,
    ) -> Result<RunOutput, AudioError> {
        let output = run(handle, args, None, self.timeout)
            .await
            .map_err(|source| AudioError::Run { tool, source })?;
        if output.exit.success() {
            Ok(output)
        } else {
            Err(AudioError::Failed {
                tool,
                exit: output.exit,
                stderr: tail(&output.stderr.text),
            })
        }
    }
}

#[derive(Deserialize)]
struct Report {
    #[serde(default)]
    streams: Vec<IgnoredAny>,
    #[serde(default)]
    format: Format,
}

#[derive(Default, Deserialize)]
struct Format {
    duration: Option<String>,
}

fn tail(text: &str) -> String {
    let text = text.trim();
    let skip = text.chars().count().saturating_sub(STDERR_TAIL_CHARS);
    text.chars().skip(skip).collect()
}
