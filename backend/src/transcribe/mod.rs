mod audio;
mod speech;

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

pub use audio::AudioError;
pub use speech::SpeechError;

use crate::config::Config;
use crate::tools::{Tool, ToolSet};
use audio::AudioTools;
use speech::SpeechClient;

const AUDIO_FILE: &str = "audio.mp3";
const TRANSCRIPT_FILE: &str = "transcript.txt";

/// Covers a local Whisper on a CPU working through a long reel, and matches the 300 s
/// Mealie allows its own AI.
pub const SPEECH_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub enum Outcome {
    Skipped(Skip),
    Done(Transcript),
    Failed(Failure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    NoSpeechApi,
    MissingTool(Tool),
    NoVideo,
    NoAudioTrack,
}

#[derive(Debug)]
pub struct Transcript {
    /// Empty when the audio holds no speech, as in a music-only reel.
    pub text: String,
    pub file: PathBuf,
    pub audio_duration: Duration,
}

/// `Display` is fixed text. `Debug` holds the whole cause chain, which never includes the API key.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("audio extraction failed")]
    Audio(#[source] AudioError),
    #[error("the speech API call failed")]
    Speech(#[source] SpeechError),
    #[error("the transcript could not be saved")]
    Save(#[source] io::Error),
}

#[async_trait]
pub trait Transcriber: Send + Sync {
    /// `video` is `None` for a photo-only post. On `Done`, `transcript.txt` is in `import_dir`,
    /// even when empty.
    async fn transcribe(&self, video: Option<&Path>, import_dir: &Path) -> Outcome;
}

#[derive(Debug)]
pub struct SpeechTranscriber {
    /// Settled once at startup, because the configuration and the tools do not change while
    /// the backend runs.
    setup: Result<Ready, Skip>,
}

#[derive(Debug)]
struct Ready {
    speech: SpeechClient,
    audio: AudioTools,
}

impl SpeechTranscriber {
    pub fn new(
        config: &Config,
        tools: &ToolSet,
        speech_timeout: Duration,
    ) -> Result<Self, reqwest::Error> {
        let Some(speech) = &config.transcription else {
            tracing::info!("transcription is off, TRANSCRIPTION_BASE_URL is not set");
            return Ok(Self {
                setup: Err(Skip::NoSpeechApi),
            });
        };
        let audio = match AudioTools::from_tool_set(tools, config.tool_timeout) {
            Ok(audio) => audio,
            Err(tool) => {
                tracing::warn!(%tool, "transcription is off, the tool was not found");
                return Ok(Self {
                    setup: Err(Skip::MissingTool(tool)),
                });
            }
        };
        Ok(Self {
            setup: Ok(Ready {
                speech: SpeechClient::new(speech, speech_timeout)?,
                audio,
            }),
        })
    }
}

#[async_trait]
impl Transcriber for SpeechTranscriber {
    async fn transcribe(&self, video: Option<&Path>, import_dir: &Path) -> Outcome {
        let ready = match &self.setup {
            Ok(ready) => ready,
            Err(reason) => return Outcome::Skipped(*reason),
        };
        let Some(video) = video else {
            return Outcome::Skipped(Skip::NoVideo);
        };
        let audio_file = import_dir.join(AUDIO_FILE);
        let audio_duration = match ready.audio.extract(video, &audio_file).await {
            Ok(Some(duration)) => duration,
            Ok(None) => return Outcome::Skipped(Skip::NoAudioTrack),
            Err(error) => return Outcome::Failed(Failure::Audio(error)),
        };
        let text = match ready.speech.transcribe(&audio_file).await {
            Ok(text) => text,
            Err(error) => return Outcome::Failed(Failure::Speech(error)),
        };
        let file = import_dir.join(TRANSCRIPT_FILE);
        if let Err(error) = tokio::fs::write(&file, &text).await {
            return Outcome::Failed(Failure::Save(error));
        }
        Outcome::Done(Transcript {
            text,
            file,
            audio_duration,
        })
    }
}
