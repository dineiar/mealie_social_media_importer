use std::collections::HashMap;
use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mealie_importer::config::Config;
use mealie_importer::process::run;
use mealie_importer::tools::{Tool, ToolSet};
use mealie_importer::transcribe::{
    AudioError, Failure, Outcome, SPEECH_TIMEOUT, Skip, SpeechError, SpeechTranscriber,
    Transcriber, Transcript,
};
use reqwest::StatusCode;
use serde_json::json;
use tempfile::{TempDir, tempdir};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

mod common;

use common::install;

const KEY: &str = "sk-live-0123456789";

const SPOKEN: &str = r#"{"streams":[{"codec_type":"audio"}],"format":{"duration":"47.5"}}"#;
const SILENT: &str = r#"{"streams":[],"format":{"duration":"12.0"}}"#;

const FFMPEG_STUB: &str = r#"#!/bin/sh
printf '%s\n' "$@" >"${0%/*}/ffmpeg.args"
for last; do :; done
printf 'ID3-stub-audio' >"$last"
"#;

const FFMPEG_BROKEN: &str = r#"#!/bin/sh
echo "Invalid data found when processing input" >&2
exit 183
"#;

fn ffprobe_stub(report: &str) -> String {
    r#"#!/bin/sh
printf '%s\n' "$@" >"${0%/*}/ffprobe.args"
cat <<'REPORT'
<report>
REPORT
"#
    .replace("<report>", report)
}

struct Rig {
    tmp: TempDir,
    import: PathBuf,
    tools: ToolSet,
    video: PathBuf,
}

impl Rig {
    fn new() -> Self {
        Self::with_stubs(FFMPEG_STUB, SPOKEN)
    }

    fn with_stubs(ffmpeg: &str, report: &str) -> Self {
        Self::with_scripts(Some(ffmpeg), Some(&ffprobe_stub(report)))
    }

    fn with_scripts(ffmpeg: Option<&str>, ffprobe: Option<&str>) -> Self {
        let tmp = tempdir().unwrap();
        let bin = tmp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for (name, script) in [("ffmpeg", ffmpeg), ("ffprobe", ffprobe)] {
            if let Some(script) = script {
                install(&bin, name, script);
            }
        }
        let tools = ToolSet::discover(tmp.path(), Some(bin.as_os_str()));
        let import = tmp.path().join("import");
        fs::create_dir(&import).unwrap();
        let video = import.join("video-1.mp4");
        fs::write(&video, "not really a video").unwrap();
        Self {
            tmp,
            import,
            tools,
            video,
        }
    }

    fn args(&self, tool: &str) -> Vec<String> {
        fs::read_to_string(self.tmp.path().join("bin").join(format!("{tool}.args")))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn transcript_file(&self) -> PathBuf {
        self.import.join("transcript.txt")
    }

    async fn transcribe_via(&self, server: &MockServer) -> Outcome {
        self.transcribe(&config_for(server, &[])).await
    }

    async fn transcribe(&self, config: &Config) -> Outcome {
        self.transcribe_within(config, SPEECH_TIMEOUT).await
    }

    async fn transcribe_within(&self, config: &Config, timeout: Duration) -> Outcome {
        SpeechTranscriber::new(config, &self.tools, timeout)
            .unwrap()
            .transcribe(Some(&self.video), &self.import)
            .await
    }
}

fn config(vars: &[(&str, &str)]) -> Config {
    let mut values: HashMap<String, String> = [
        ("MEALIE_URL", "http://mealie.test"),
        ("TOOL_TIMEOUT_SECS", "10"),
    ]
    .into_iter()
    .chain(vars.iter().copied())
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect();
    values.retain(|_, value| !value.is_empty());
    Config::from_vars(&values).unwrap()
}

fn config_for(server: &MockServer, extra: &[(&str, &str)]) -> Config {
    let base = format!("{}/v1", server.uri());
    let mut vars = vec![
        ("TRANSCRIPTION_BASE_URL", base.as_str()),
        ("TRANSCRIPTION_MODEL", "whisper-test"),
    ];
    vars.extend_from_slice(extra);
    config(&vars)
}

async fn replying(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

async fn replying_text(text: &str) -> MockServer {
    replying(ResponseTemplate::new(200).set_body_json(json!({ "text": text }))).await
}

async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

async fn single_request(server: &MockServer) -> Request {
    let mut sent = requests(server).await;
    assert_eq!(sent.len(), 1, "expected one request");
    sent.remove(0)
}

fn done(outcome: Outcome) -> Transcript {
    match outcome {
        Outcome::Done(transcript) => transcript,
        other => panic!("expected a transcript, got {other:?}"),
    }
}

fn skipped(outcome: Outcome) -> Skip {
    match outcome {
        Outcome::Skipped(skip) => skip,
        other => panic!("expected the step to be skipped, got {other:?}"),
    }
}

fn failed(outcome: Outcome) -> Failure {
    match outcome {
        Outcome::Failed(failure) => failure,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn speech_error(outcome: Outcome) -> SpeechError {
    match failed(outcome) {
        Failure::Speech(error) => error,
        other => panic!("expected a speech API failure, got {other:?}"),
    }
}

#[tokio::test]
async fn speech_becomes_a_trimmed_transcript_in_the_import_directory() {
    let server = replying_text(" Start with two tablespoons of butter.\n").await;
    let rig = Rig::new();

    let transcript = done(rig.transcribe_via(&server).await);

    assert_eq!(transcript.text, "Start with two tablespoons of butter.");
    assert_eq!(transcript.file, rig.transcript_file());
    assert_eq!(
        fs::read_to_string(rig.transcript_file()).unwrap(),
        "Start with two tablespoons of butter."
    );
    assert_eq!(transcript.audio_duration, Duration::from_secs_f64(47.5));
}

#[tokio::test]
async fn fields_besides_text_are_ignored() {
    let server = replying(ResponseTemplate::new(200).set_body_json(json!({
        "text": "Hello.",
        "language": "english",
        "duration": 47.5,
        "segments": [{ "id": 0, "text": "Hello." }],
    })))
    .await;
    let rig = Rig::new();

    let transcript = done(rig.transcribe_via(&server).await);

    assert_eq!(transcript.text, "Hello.");
}

#[tokio::test]
async fn an_empty_transcript_is_a_result_and_still_gets_its_file() {
    for reply in ["", "  \n"] {
        let server = replying_text(reply).await;
        let rig = Rig::new();

        let transcript = done(rig.transcribe_via(&server).await);

        assert_eq!(transcript.text, "");
        assert_eq!(fs::read_to_string(rig.transcript_file()).unwrap(), "");
    }
}

#[tokio::test]
async fn an_error_status_fails_the_step_with_that_status() {
    for code in [400, 401, 404, 413, 422, 429, 500, 502, 503] {
        let server = replying(ResponseTemplate::new(code).set_body_string("model crashed")).await;
        let rig = Rig::new();

        let error = speech_error(rig.transcribe_via(&server).await);

        let SpeechError::Status(status) = error else {
            panic!("expected a status error for {code}, got {error:?}");
        };
        assert_eq!(status.as_u16(), code);
        assert!(!rig.transcript_file().exists());
    }
}

#[tokio::test]
async fn a_server_slower_than_the_timeout_fails_the_step() {
    let server = replying(
        ResponseTemplate::new(200)
            .set_body_json(json!({ "text": "too late" }))
            .set_delay(Duration::from_secs(3)),
    )
    .await;
    let rig = Rig::new();

    let outcome = rig
        .transcribe_within(&config_for(&server, &[]), Duration::from_millis(200))
        .await;

    let SpeechError::Unreachable(error) = speech_error(outcome) else {
        panic!("a timeout is an unreachable server");
    };
    assert!(error.is_timeout(), "{error:?}");
    assert!(!rig.transcript_file().exists());
}

#[tokio::test]
async fn a_server_that_is_not_listening_fails_the_step() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let base = format!("http://127.0.0.1:{port}/v1");
    let config = config(&[("TRANSCRIPTION_BASE_URL", &base)]);
    let rig = Rig::new();

    let error = speech_error(rig.transcribe(&config).await);

    let SpeechError::Unreachable(error) = error else {
        panic!("expected an unreachable server, got {error:?}");
    };
    assert!(error.is_connect(), "{error:?}");
}

#[tokio::test]
async fn a_reply_that_is_not_a_transcript_fails_the_step() {
    let oversized = format!(r#"{{"text":"{}"}}"#, "a".repeat(2 * 1024 * 1024));
    let replies = [
        ResponseTemplate::new(200).set_body_string("Okay, start with butter."),
        ResponseTemplate::new(200).set_body_raw("<html>Sign in</html>", "text/html"),
        ResponseTemplate::new(200).set_body_json(json!({ "words": [] })),
        ResponseTemplate::new(200).set_body_json(json!({ "text": null })),
        ResponseTemplate::new(200).set_body_json(json!(["text"])),
        ResponseTemplate::new(200).set_body_string(oversized),
        ResponseTemplate::new(200),
    ];
    for reply in replies {
        let server = replying(reply).await;
        let rig = Rig::new();

        let error = speech_error(rig.transcribe_via(&server).await);

        assert!(matches!(error, SpeechError::BadReply), "{error:?}");
        assert!(!rig.transcript_file().exists());
    }
}

#[tokio::test]
async fn a_redirect_is_not_followed() {
    let server = replying(
        ResponseTemplate::new(307).insert_header("location", "/elsewhere/audio/transcriptions"),
    )
    .await;
    let rig = Rig::new();

    let error = speech_error(
        rig.transcribe(&config_for(&server, &[("TRANSCRIPTION_API_KEY", KEY)]))
            .await,
    );

    assert!(matches!(
        error,
        SpeechError::Status(StatusCode::TEMPORARY_REDIRECT)
    ));
    assert_eq!(requests(&server).await.len(), 1);
}

struct Field {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
    body: String,
}

fn fields(request: &Request) -> Vec<Field> {
    let content_type = request
        .headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    let (kind, boundary) = content_type.split_once("boundary=").unwrap();
    assert!(kind.starts_with("multipart/form-data"), "{content_type}");
    let body = String::from_utf8_lossy(&request.body).into_owned();
    let delimiter = format!("--{boundary}");
    body.split(&delimiter)
        .skip(1)
        .filter(|chunk| !chunk.starts_with("--"))
        .map(|chunk| {
            let (head, body) = chunk
                .trim_start_matches("\r\n")
                .split_once("\r\n\r\n")
                .unwrap();
            let header = |wanted: &str| {
                head.lines().find_map(|line| {
                    let (name, value) = line.split_once(": ")?;
                    name.eq_ignore_ascii_case(wanted).then(|| value.to_owned())
                })
            };
            let disposition = header("content-disposition").unwrap();
            let attribute = |key: &str| {
                let (_, rest) = disposition.split_once(&format!("; {key}=\""))?;
                Some(rest.split('"').next().unwrap().to_owned())
            };
            Field {
                name: attribute("name").unwrap(),
                filename: attribute("filename"),
                content_type: header("content-type"),
                body: body.strip_suffix("\r\n").unwrap().to_owned(),
            }
        })
        .collect()
}

fn field<'a>(fields: &'a [Field], name: &str) -> &'a Field {
    fields.iter().find(|field| field.name == name).unwrap()
}

fn names(fields: &[Field]) -> Vec<&str> {
    let mut names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    names.sort_unstable();
    names
}

#[tokio::test]
async fn the_request_carries_the_documented_fields_and_the_configured_options() {
    let server = replying_text("ok").await;
    let rig = Rig::new();
    let config = config_for(
        &server,
        &[
            ("TRANSCRIPTION_LANGUAGE", "pt"),
            ("TRANSCRIPTION_API_KEY", KEY),
        ],
    );

    done(rig.transcribe(&config).await);

    let request = single_request(&server).await;
    assert_eq!(request.url.path(), "/v1/audio/transcriptions");
    assert_eq!(
        request
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("Bearer {KEY}")
    );
    let fields = fields(&request);
    assert_eq!(
        names(&fields),
        ["file", "language", "model", "response_format"]
    );
    assert_eq!(field(&fields, "model").body, "whisper-test");
    assert_eq!(field(&fields, "language").body, "pt");
    assert_eq!(field(&fields, "response_format").body, "json");
    let file = field(&fields, "file");
    assert_eq!(file.filename.as_deref(), Some("audio.mp3"));
    assert_eq!(file.content_type.as_deref(), Some("audio/mpeg"));
    assert_eq!(file.body, "ID3-stub-audio");
}

#[tokio::test]
async fn language_and_authorization_are_left_out_when_not_configured() {
    let server = replying_text("ok").await;
    let rig = Rig::new();

    done(rig.transcribe_via(&server).await);

    let request = single_request(&server).await;
    assert!(request.headers.get("authorization").is_none());
    assert_eq!(
        names(&fields(&request)),
        ["file", "model", "response_format"]
    );
}

#[tokio::test]
async fn the_key_stays_out_of_every_error_even_when_the_server_echoes_it() {
    let echo = format!(r#"{{"error":{{"message":"Incorrect API key provided: {KEY}"}}}}"#);
    let server = replying(ResponseTemplate::new(401).set_body_string(echo)).await;
    let rig = Rig::new();
    let config = config_for(&server, &[("TRANSCRIPTION_API_KEY", KEY)]);

    let failure = failed(rig.transcribe(&config).await);

    assert!(!format!("{failure}").contains(KEY));
    assert!(!format!("{failure:?}").contains(KEY));
    assert!(!format!("{config:?}").contains(KEY));
}

#[tokio::test]
async fn a_key_that_cannot_go_in_a_header_fails_the_step_and_is_not_sent_or_shown() {
    let bad_key = "sk-line\nbreak";
    let server = replying_text("never").await;
    let rig = Rig::new();
    let config = config_for(&server, &[("TRANSCRIPTION_API_KEY", bad_key)]);

    let error = speech_error(rig.transcribe(&config).await);

    assert!(matches!(error, SpeechError::Build(_)), "{error:?}");
    assert!(!format!("{error:?}").contains("sk-line"));
    assert!(requests(&server).await.is_empty());
}

#[tokio::test]
async fn ffmpeg_and_ffprobe_get_exactly_these_arguments() {
    let server = replying_text("ok").await;
    let rig = Rig::new();

    done(rig.transcribe_via(&server).await);

    let video = rig.video.to_str().unwrap();
    let audio = rig.import.join("audio.mp3");
    assert_eq!(
        rig.args("ffprobe"),
        [
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_type:format=duration",
            "-of",
            "json",
            "-i",
            video,
        ]
    );
    assert_eq!(
        rig.args("ffmpeg"),
        [
            "-nostdin",
            "-loglevel",
            "error",
            "-y",
            "-i",
            video,
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-codec:a",
            "libmp3lame",
            "-b:a",
            "32k",
            audio.to_str().unwrap(),
        ]
    );
}

#[tokio::test]
async fn without_a_speech_api_the_step_is_skipped() {
    let rig = Rig::new();

    let skip = skipped(rig.transcribe(&config(&[])).await);

    assert_eq!(skip, Skip::NoSpeechApi);
    assert!(!rig.tmp.path().join("bin/ffprobe.args").exists());
}

#[tokio::test]
async fn a_post_without_video_is_skipped_without_calling_anyone() {
    let server = replying_text("never").await;
    let rig = Rig::new();
    let transcriber =
        SpeechTranscriber::new(&config_for(&server, &[]), &rig.tools, SPEECH_TIMEOUT).unwrap();

    let skip = skipped(transcriber.transcribe(None, &rig.import).await);

    assert_eq!(skip, Skip::NoVideo);
    assert!(requests(&server).await.is_empty());
    assert!(!rig.tmp.path().join("bin/ffprobe.args").exists());
}

#[tokio::test]
async fn a_missing_ffmpeg_or_ffprobe_means_not_configured() {
    let ffprobe = ffprobe_stub(SPOKEN);
    for (rig, missing) in [
        (Rig::with_scripts(None, Some(&ffprobe)), Tool::Ffmpeg),
        (Rig::with_scripts(Some(FFMPEG_STUB), None), Tool::Ffprobe),
    ] {
        let server = replying_text("never").await;

        let skip = skipped(rig.transcribe_via(&server).await);

        assert_eq!(skip, Skip::MissingTool(missing));
        assert!(requests(&server).await.is_empty());
    }
}

#[tokio::test]
async fn a_video_without_an_audio_track_is_skipped_without_calling_the_speech_api() {
    let server = replying_text("never").await;
    let rig = Rig::with_stubs(FFMPEG_STUB, SILENT);

    let skip = skipped(rig.transcribe_via(&server).await);

    assert_eq!(skip, Skip::NoAudioTrack);
    assert!(requests(&server).await.is_empty());
    assert!(!rig.tmp.path().join("bin/ffmpeg.args").exists());
}

#[tokio::test]
async fn a_failing_ffmpeg_fails_the_step_without_calling_the_speech_api() {
    let server = replying_text("never").await;
    let rig = Rig::with_stubs(FFMPEG_BROKEN, SPOKEN);

    let failure = failed(rig.transcribe_via(&server).await);

    let Failure::Audio(AudioError::Failed { tool, exit, stderr }) = failure else {
        panic!("expected ffmpeg to fail, got {failure:?}");
    };
    assert_eq!(tool, Tool::Ffmpeg);
    assert_eq!(exit.to_string(), "exit code 183");
    assert_eq!(stderr, "Invalid data found when processing input");
    assert!(requests(&server).await.is_empty());
}

#[tokio::test]
async fn an_ffprobe_report_without_a_duration_fails_the_step() {
    for report in [
        r#"{"streams":[{"codec_type":"audio"}],"format":{"duration":"N/A"}}"#,
        r#"{"streams":[{"codec_type":"audio"}],"format":{}}"#,
        r#"{"streams":[{"codec_type":"audio"}],"format":{"duration":"-3"}}"#,
        "not json",
    ] {
        let server = replying_text("never").await;
        let rig = Rig::with_stubs(FFMPEG_STUB, report);

        let failure = failed(rig.transcribe_via(&server).await);

        assert!(
            matches!(failure, Failure::Audio(AudioError::UnusableProbe)),
            "{report}: {failure:?}"
        );
        assert!(requests(&server).await.is_empty());
    }
}

#[tokio::test]
async fn a_transcript_that_cannot_be_saved_fails_the_step() {
    let server = replying_text("ok").await;
    let rig = Rig::new();
    fs::create_dir(rig.transcript_file()).unwrap();

    let failure = failed(rig.transcribe_via(&server).await);

    assert!(matches!(failure, Failure::Save(_)), "{failure:?}");
}

#[tokio::test]
async fn running_again_replaces_the_earlier_transcript() {
    let first = replying_text("first").await;
    let second = replying_text("second").await;
    let rig = Rig::new();

    done(rig.transcribe(&config_for(&first, &[])).await);
    done(rig.transcribe(&config_for(&second, &[])).await);

    assert_eq!(fs::read_to_string(rig.transcript_file()).unwrap(), "second");
}

async fn tool_stdout(tools: &ToolSet, tool: Tool, args: &[&str]) -> String {
    let output = run(
        tools.get(tool).expect("the tool is installed"),
        args,
        None,
        Duration::from_secs(300),
    )
    .await
    .unwrap();
    assert!(output.exit.success(), "{tool}: {}", output.stderr.text);
    output.stdout.text
}

fn discovered_tools(data: &Path) -> ToolSet {
    ToolSet::discover(data, env::var_os("PATH").as_deref())
}

async fn real_tool_run(tmp: &Path, make_video: &[&str], video: &str) -> (ToolSet, PathBuf) {
    let tools = discovered_tools(tmp);
    let import = tmp.join("import");
    fs::create_dir(&import).unwrap();
    let video = import.join(video);
    let mut args = vec!["-v", "error", "-y"];
    args.extend_from_slice(make_video);
    args.push(video.to_str().unwrap());
    tool_stdout(&tools, Tool::Ffmpeg, &args).await;
    (tools, video)
}

#[tokio::test]
#[ignore = "needs real ffmpeg and ffprobe on PATH"]
async fn ten_minutes_of_video_become_less_than_three_megabytes_of_mono_mp3() {
    let tmp = tempdir().unwrap();
    let (tools, video) = real_tool_run(
        tmp.path(),
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=5",
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=color=pink:sample_rate=48000:amplitude=0.4",
            "-ac",
            "2",
            "-t",
            "600",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        "ten-minutes.mp4",
    )
    .await;
    let import = video.parent().unwrap();
    let server = replying_text("ok").await;
    let transcriber =
        SpeechTranscriber::new(&config_for(&server, &[]), &tools, SPEECH_TIMEOUT).unwrap();

    let transcript = done(transcriber.transcribe(Some(&video), import).await);

    let audio = import.join("audio.mp3");
    let size = fs::metadata(&audio).unwrap().len();
    let sent = single_request(&server).await.body.len();
    let described = tool_stdout(
        &tools,
        Tool::Ffprobe,
        &[
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,channels,bit_rate:format=duration,size",
            "-of",
            "default=noprint_wrappers=1",
            audio.to_str().unwrap(),
        ],
    )
    .await;
    println!(
        "input video: {} bytes, {:?} by ffprobe",
        fs::metadata(&video).unwrap().len(),
        transcript.audio_duration
    );
    println!(
        "audio.mp3: {size} bytes ({:.2} MB), request body {sent} bytes",
        size as f64 / 1_000_000.0
    );
    println!("ffprobe of audio.mp3:\n{described}");
    assert!(size < 3_000_000, "{size} bytes");
    assert!(sent < 3_000_000, "{sent} bytes");
    assert!(described.contains("codec_name=mp3"), "{described}");
    assert!(described.contains("channels=1"), "{described}");
    assert!(described.contains("bit_rate=32000"), "{described}");
    assert!((transcript.audio_duration.as_secs_f64() - 600.0).abs() < 1.0);
}

#[tokio::test]
#[ignore = "needs real ffmpeg and ffprobe on PATH"]
async fn a_video_with_no_audio_track_is_skipped_by_the_real_ffprobe() {
    let tmp = tempdir().unwrap();
    let (tools, video) = real_tool_run(
        tmp.path(),
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=5",
            "-t",
            "2",
            "-pix_fmt",
            "yuv420p",
        ],
        "silent.mp4",
    )
    .await;
    let server = replying_text("never").await;
    let transcriber =
        SpeechTranscriber::new(&config_for(&server, &[]), &tools, SPEECH_TIMEOUT).unwrap();

    let skip = skipped(
        transcriber
            .transcribe(Some(&video), video.parent().unwrap())
            .await,
    );

    assert_eq!(skip, Skip::NoAudioTrack);
    assert!(requests(&server).await.is_empty());
}

/// Run by hand against a real OpenAI-compatible server, for example a speaches container:
///
/// ```sh
/// LIVE_TRANSCRIPTION_BASE_URL=http://127.0.0.1:8000/v1 \
/// LIVE_TRANSCRIPTION_MODEL=Systran/faster-whisper-tiny.en \
/// LIVE_CLIP=/path/to/clip.mp4 LIVE_EXPECTED_WORDS="butter garlic" \
/// cargo test --test transcribe live_ -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "needs a running speech server, see the LIVE_ variables in the doc comment"]
async fn live_server_transcribes_a_spoken_clip() {
    let var = |name: &str| env::var(name).ok().filter(|value| !value.is_empty());
    let base = var("LIVE_TRANSCRIPTION_BASE_URL").expect("LIVE_TRANSCRIPTION_BASE_URL");
    let model = var("LIVE_TRANSCRIPTION_MODEL").expect("LIVE_TRANSCRIPTION_MODEL");
    let clip = PathBuf::from(var("LIVE_CLIP").expect("LIVE_CLIP"));
    let key = var("LIVE_TRANSCRIPTION_API_KEY").unwrap_or_default();
    let language = var("LIVE_TRANSCRIPTION_LANGUAGE").unwrap_or_default();
    let vars = [
        ("TRANSCRIPTION_BASE_URL", base.as_str()),
        ("TRANSCRIPTION_MODEL", model.as_str()),
        ("TRANSCRIPTION_API_KEY", key.as_str()),
        ("TRANSCRIPTION_LANGUAGE", language.as_str()),
    ];
    let tmp = tempdir().unwrap();
    let import = tmp.path().join("import");
    fs::create_dir(&import).unwrap();
    let video = import.join(clip.file_name().unwrap());
    fs::copy(&clip, &video).unwrap();
    let transcriber = SpeechTranscriber::new(
        &config(&vars),
        &discovered_tools(tmp.path()),
        SPEECH_TIMEOUT,
    )
    .unwrap();

    let started = std::time::Instant::now();
    let transcript = done(transcriber.transcribe(Some(&video), &import).await);

    println!(
        "transcribed {:?} of audio in {:?}",
        transcript.audio_duration,
        started.elapsed()
    );
    println!("transcript: {:?}", transcript.text);
    assert_eq!(
        fs::read_to_string(&transcript.file).unwrap(),
        transcript.text
    );
    let heard = transcript.text.to_lowercase();
    for word in var("LIVE_EXPECTED_WORDS")
        .unwrap_or_default()
        .split_whitespace()
    {
        assert!(
            heard.contains(&word.to_lowercase()),
            "{word:?} not in {heard:?}"
        );
    }
    assert!(!transcript.text.is_empty());
}
