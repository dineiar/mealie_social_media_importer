use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mealie_importer::tools::{Tool, ToolSet};
use tempfile::{Builder, NamedTempFile, TempDir, tempdir};

mod common;

use common::install;

fn venv_bin(data: &Path) -> PathBuf {
    data.join("tools/bin")
}

fn discover(data: &Path, bin: &Path) -> ToolSet {
    ToolSet::discover(data, Some(bin.as_os_str()))
}

#[test]
fn the_virtualenv_wins_over_path() {
    let data = tempdir().unwrap();
    let bin = tempdir().unwrap();
    let venv_tool = install(&venv_bin(data.path()), "gallery-dl", "");
    install(bin.path(), "gallery-dl", "");
    install(bin.path(), "ffmpeg", "");

    let tools = discover(data.path(), bin.path());

    assert_eq!(tools.path(Tool::GalleryDl), Some(venv_tool.as_path()));
    assert_eq!(
        tools.path(Tool::Ffmpeg),
        Some(bin.path().join("ffmpeg").as_path())
    );
}

#[test]
fn a_tool_that_is_nowhere_is_missing() {
    let data = tempdir().unwrap();
    let bin = tempdir().unwrap();
    install(bin.path(), "ffprobe", "");

    let tools = discover(data.path(), bin.path());

    assert!(tools.path(Tool::Ffprobe).is_some());
    assert_eq!(tools.path(Tool::GalleryDl), None);
    assert_eq!(tools.path(Tool::YtDlp), None);
    assert_eq!(tools.path(Tool::Ffmpeg), None);
}

#[test]
fn a_file_that_cannot_execute_or_a_directory_is_not_a_tool() {
    let data = tempdir().unwrap();
    let bin = tempdir().unwrap();
    fs::write(bin.path().join("ffmpeg"), "").unwrap();
    fs::create_dir(bin.path().join("ffprobe")).unwrap();

    let tools = discover(data.path(), bin.path());

    assert_eq!(tools.path(Tool::Ffmpeg), None);
    assert_eq!(tools.path(Tool::Ffprobe), None);
}

fn relative_tempdir() -> (TempDir, PathBuf) {
    let dir = Builder::new()
        .prefix("tools-test-")
        .tempdir_in(".")
        .unwrap();
    let cwd = env::current_dir().unwrap();
    let relative = dir.path().strip_prefix(cwd).unwrap().to_owned();
    (dir, relative)
}

#[test]
fn a_relative_data_dir_still_yields_absolute_tool_paths() {
    let (data, relative) = relative_tempdir();
    install(&venv_bin(data.path()), "yt-dlp", "");

    let tools = ToolSet::discover(&relative, None);

    let found = tools.path(Tool::YtDlp).unwrap();
    assert!(found.is_absolute());
    assert!(found.is_file());
}

#[test]
fn a_relative_path_entry_is_not_searched() {
    let data = tempdir().unwrap();
    let (bin, relative) = relative_tempdir();
    install(bin.path(), "ffmpeg", "");

    let tools = discover(data.path(), &relative);

    assert_eq!(tools.path(Tool::Ffmpeg), None);
}

#[test]
fn no_path_variable_leaves_only_the_virtualenv() {
    let data = tempdir().unwrap();
    install(&venv_bin(data.path()), "gallery-dl", "");

    let tools = ToolSet::discover(data.path(), None);

    assert!(tools.path(Tool::GalleryDl).is_some());
    assert_eq!(tools.path(Tool::YtDlp), None);
}

#[tokio::test]
async fn the_version_log_names_each_tool_and_survives_one_that_misbehaves() {
    let data = tempdir().unwrap();
    let bin = tempdir().unwrap();
    install(
        &venv_bin(data.path()),
        "gallery-dl",
        "#!/bin/sh\necho 1.31.0\n",
    );
    install(bin.path(), "yt-dlp", "#!/bin/sh\nexec sleep 300\n");
    install(bin.path(), "ffmpeg", "#!/bin/sh\nexit 3\n");
    let tools = discover(data.path(), bin.path());
    let log = NamedTempFile::new().unwrap();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(log.reopen().unwrap())
        .with_ansi(false)
        .finish();
    let _logging = tracing::subscriber::set_default(subscriber);

    tokio::time::timeout(
        Duration::from_secs(10),
        tools.log_versions(Duration::from_secs(1)),
    )
    .await
    .expect("a hanging version check held up the log");

    let text = fs::read_to_string(log.path()).unwrap();
    let line_for = |tool: &str| {
        text.lines()
            .find(|line| line.contains(tool))
            .unwrap_or_else(|| panic!("no log line mentions {tool}:\n{text}"))
    };
    for (tool, level, detail) in [
        ("gallery-dl", "INFO", "1.31.0"),
        ("yt-dlp", "WARN", "timed out"),
        ("ffmpeg", "WARN", "exit code 3"),
        ("ffprobe", "WARN", ""),
    ] {
        let line = line_for(tool);
        assert!(line.contains(level) && line.contains(detail), "{line}");
    }
}
