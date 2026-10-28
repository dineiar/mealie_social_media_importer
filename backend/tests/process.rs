use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mealie_importer::process::{Exit, OUTPUT_CAP, RunError, run};
use mealie_importer::tools::{Tool, ToolSet};
use rustix::process::{Pid, test_kill_process};
use tempfile::tempdir;
use tokio::task::JoinSet;

mod common;
#[path = "common/stub.rs"]
mod stub_tool;

use common::install;
use stub_tool::stub;

const LONG: Duration = Duration::from_secs(60);

fn is_running(pid: i32) -> bool {
    test_kill_process(Pid::from_raw(pid).unwrap()).is_ok()
}

async fn until<T>(failure: &str, mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(Instant::now() < deadline, "{failure}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

struct Helper {
    pid_file: PathBuf,
    ticks: PathBuf,
}

impl Helper {
    fn in_dir(dir: &Path) -> Self {
        Self {
            pid_file: dir.join("pid"),
            ticks: dir.join("ticks"),
        }
    }

    fn args(&self, mode: &str) -> [OsString; 3] {
        [
            mode.into(),
            self.pid_file.clone().into(),
            self.ticks.clone().into(),
        ]
    }

    async fn pid(&self) -> i32 {
        until("the stub never wrote its pid", || {
            fs::read_to_string(&self.pid_file).ok()?.trim().parse().ok()
        })
        .await
    }

    async fn assert_helper_stopped(&self) {
        let ticks = || fs::metadata(&self.ticks).unwrap().len();
        let mut last_change = (ticks(), Instant::now());
        until("a helper the child started is still running", || {
            let now = ticks();
            if now != last_change.0 {
                last_change = (now, Instant::now());
            }
            (last_change.1.elapsed() >= Duration::from_millis(300)).then_some(())
        })
        .await;
    }
}

#[tokio::test]
async fn a_successful_run_returns_its_output() {
    let out = run(&stub(), ["speak", "hello world"], None, LONG)
        .await
        .unwrap();

    assert_eq!(out.exit, Exit::Code(0));
    assert_eq!(out.stdout.text, "out hello world\n");
    assert_eq!(out.stderr.text, "err hello world\n");
    assert!(!out.stdout.truncated && !out.stderr.truncated);
}

#[tokio::test]
async fn a_non_zero_exit_keeps_its_code_and_output() {
    let out = run(&stub(), ["exit", "3"], None, LONG).await.unwrap();

    assert_eq!(out.exit, Exit::Code(3));
    assert!(!out.exit.success());
    assert_eq!(out.stdout.text, "partial\n");
}

#[tokio::test]
async fn a_child_killed_by_a_signal_is_not_a_timeout() {
    let out = run(&stub(), ["die"], None, LONG).await.unwrap();

    assert_eq!(out.exit, Exit::Signal(9));
}

#[tokio::test]
async fn arguments_reach_the_program_without_shell_interpretation() {
    let hostile = "$(echo pwned) `echo pwned`; * | > /dev/null";

    let out = run(&stub(), ["speak", hostile], None, LONG).await.unwrap();

    assert_eq!(out.stdout.text, format!("out {hostile}\n"));
}

#[tokio::test]
async fn the_child_runs_in_the_given_directory() {
    let dir = tempdir().unwrap();

    let out = run(&stub(), ["cwd"], Some(dir.path()), LONG).await.unwrap();

    assert_eq!(
        Path::new(out.stdout.text.trim()).canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[tokio::test]
async fn output_over_the_cap_is_truncated_and_the_child_still_finishes() {
    let out = run(&stub(), ["flood"], None, Duration::from_secs(30))
        .await
        .unwrap();

    assert_eq!(out.exit, Exit::Code(0));
    assert_eq!(out.stdout.text.len(), OUTPUT_CAP);
    assert_eq!(out.stderr.text.len(), OUTPUT_CAP);
    assert!(out.stdout.truncated && out.stderr.truncated);
}

#[tokio::test]
async fn a_timeout_kills_the_child_and_everything_it_started() {
    let dir = tempdir().unwrap();
    let helper = Helper::in_dir(dir.path());

    let out = run(&stub(), helper.args("hang"), None, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(out.exit, Exit::TimedOut);
    assert_eq!(out.stdout.text, "started\n");
    assert!(!is_running(helper.pid().await));
    helper.assert_helper_stopped().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timed_out_child_is_reaped_before_run_returns() {
    let dir = tempdir().unwrap();
    let mut runs = JoinSet::new();
    for n in 0..32 {
        let pid_file = dir.path().join(format!("pid-{n}"));
        runs.spawn(async move {
            let args = [OsString::from("nap"), pid_file.clone().into()];
            let out = run(&stub(), args, None, Duration::from_secs(2))
                .await
                .unwrap();
            assert_eq!(out.exit, Exit::TimedOut);
            let pid = fs::read_to_string(pid_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert!(!is_running(pid), "the child is a zombie or still alive");
        });
    }

    while let Some(finished) = runs.join_next().await {
        finished.unwrap();
    }
}

#[tokio::test]
async fn dropping_a_run_kills_the_child_and_everything_it_started() {
    let dir = tempdir().unwrap();
    let helper = Helper::in_dir(dir.path());
    let (stub, args) = (stub(), helper.args("hang"));
    let running = tokio::spawn(async move { run(&stub, args, None, LONG).await });
    let pid = helper.pid().await;
    assert!(is_running(pid));

    running.abort();

    until("the child outlived its dropped run", || {
        (!is_running(pid)).then_some(())
    })
    .await;
    helper.assert_helper_stopped().await;
}

#[tokio::test]
async fn a_helper_left_running_after_the_child_exits_is_killed() {
    let dir = tempdir().unwrap();
    let helper = Helper::in_dir(dir.path());

    let out = run(&stub(), helper.args("leave"), None, LONG)
        .await
        .unwrap();

    assert_eq!(out.exit, Exit::Code(0));
    assert_eq!(out.stdout.text, "started\n");
    helper.assert_helper_stopped().await;
}

#[tokio::test]
async fn a_program_that_cannot_start_is_a_spawn_error_naming_it_and_its_cause() {
    let bin = tempdir().unwrap();
    let path = install(bin.path(), "ffmpeg", "#!/bin/sh\n");
    let tools = ToolSet::discover(bin.path(), Some(bin.path().as_os_str()));
    fs::remove_file(&path).unwrap();

    let error = run(tools.get(Tool::Ffmpeg).unwrap(), ["-version"], None, LONG)
        .await
        .unwrap_err();

    let RunError::Spawn { program, source } = &error else {
        panic!("expected a spawn error, got {error:?}");
    };
    assert_eq!(program, &path);
    assert_eq!(source.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains("ffmpeg"), "{error}");
}
