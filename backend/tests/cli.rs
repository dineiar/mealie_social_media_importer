use std::fs;
use std::net::TcpListener;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process};
use tempfile::{NamedTempFile, tempdir};

mod common;

use common::install;

const BIN: &str = env!("CARGO_BIN_EXE_mealie-importer");

#[allow(
    clippy::disallowed_methods,
    reason = "this runs the server binary, not an external tool"
)]
fn command(vars: &[(&str, &str)], args: &[&str]) -> Command {
    let mut command = Command::new(BIN);
    command.env_clear().envs(vars.iter().copied()).args(args);
    command
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_for<T>(failure: &str, mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(Instant::now() < deadline, "{failure}");
        thread::sleep(Duration::from_millis(100));
    }
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn invalid_configuration_exits_non_zero_with_one_line_per_variable() {
    let output = command(
        &[
            ("MEALIE_TIMEOUT_SECS", "0"),
            ("BIND_ADDR", "localhost"),
            ("LOG_FORMAT", "xml"),
        ],
        &[],
    )
    .output()
    .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let vars: Vec<&str> = stderr
        .lines()
        .map(|line| line.split_whitespace().next().unwrap_or_default())
        .collect();
    assert_eq!(
        vars,
        [
            "MEALIE_URL",
            "MEALIE_TIMEOUT_SECS",
            "BIND_ADDR",
            "LOG_FORMAT"
        ]
    );
}

#[test]
fn healthcheck_follows_the_server_from_start_to_sigterm() {
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = Server(
        command(
            &[
                ("MEALIE_URL", "http://localhost:9000"),
                ("BIND_ADDR", &listen),
                ("DATA_DIR", data_dir.path().to_str().unwrap()),
            ],
            &[],
        )
        .stdout(Stdio::null())
        .spawn()
        .unwrap(),
    );
    let unspecified = format!("0.0.0.0:{port}");
    let healthcheck = || -> ExitStatus {
        command(&[("BIND_ADDR", &unspecified)], &["healthcheck"])
            .output()
            .unwrap()
            .status
    };

    wait_for("server never became healthy", || {
        healthcheck().success().then_some(())
    });

    kill_process(Pid::from_child(&server.0), Signal::TERM).unwrap();
    let exit = wait_for("server ignored SIGTERM", || server.0.try_wait().unwrap());
    assert!(exit.success(), "SIGTERM stops cleanly, got {exit}");

    assert_eq!(healthcheck().code(), Some(1));
}

#[test]
fn boot_logs_the_version_of_each_tool_found_and_warns_about_the_rest() {
    let data = tempdir().unwrap();
    install(
        &data.path().join("tools/bin"),
        "gallery-dl",
        "#!/bin/sh\necho 1.31.0\n",
    );
    let log = NamedTempFile::new().unwrap();
    let listen = format!("127.0.0.1:{}", free_port());
    let _server = Server(
        command(
            &[
                ("MEALIE_URL", "http://localhost:9000"),
                ("BIND_ADDR", &listen),
                ("DATA_DIR", data.path().to_str().unwrap()),
                ("PATH", ""),
            ],
            &[],
        )
        .stdout(log.reopen().unwrap())
        .spawn()
        .unwrap(),
    );

    let text = wait_for("the log never named every tool", || {
        let text = fs::read_to_string(log.path()).ok()?;
        ["gallery-dl", "yt-dlp", "ffmpeg", "ffprobe"]
            .iter()
            .all(|tool| text.contains(tool))
            .then_some(text)
    });

    let line_for = |tool: &str| text.lines().find(|line| line.contains(tool)).unwrap();
    let found = line_for("gallery-dl");
    assert!(
        found.contains("INFO") && found.contains("1.31.0"),
        "{found}"
    );
    let missing = line_for("yt-dlp");
    assert!(missing.contains("WARN"), "{missing}");
}
