use std::collections::HashMap;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

use mealie_importer::config::{self, Config, ConfigError, LogFormat};
use mealie_importer::{AppState, router};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => match Config::from_vars(&env_vars()) {
            Ok(config) => serve(config),
            Err(errors) => report(&errors),
        },
        [command] if command == "healthcheck" => healthcheck(),
        _ => {
            eprintln!("usage: mealie-importer [healthcheck]");
            ExitCode::from(2)
        }
    }
}

fn env_vars() -> HashMap<String, String> {
    std::env::vars_os()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

fn report(errors: &[ConfigError]) -> ExitCode {
    for error in errors {
        eprintln!("{error}");
    }
    ExitCode::FAILURE
}

#[tokio::main]
async fn serve(config: Config) -> ExitCode {
    init_logging(&config.log_filter, config.log_format);
    let addr = config.bind_addr;
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%addr, %error, "cannot listen");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(%addr, version = env!("CARGO_PKG_VERSION"), "listening");
    let app = router(AppState::new(config));
    match axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "server stopped");
            ExitCode::FAILURE
        }
    }
}

fn init_logging(filter: &str, format: LogFormat) {
    let logs = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_ansi(std::io::stdout().is_terminal());
    match format {
        LogFormat::Pretty => logs.init(),
        LogFormat::Json => logs.json().init(),
    }
}

async fn shutdown_signal() {
    let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler installs");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
    tracing::info!("shutting down");
}

fn healthcheck() -> ExitCode {
    let addr = match config::bind_addr(&env_vars()) {
        Ok(addr) => loopback_if_unspecified(addr),
        Err(errors) => return report(&errors),
    };
    match probe(addr) {
        Ok(status_line) if status_line.split_whitespace().nth(1) == Some("200") => {
            ExitCode::SUCCESS
        }
        Ok(status_line) => {
            eprintln!("healthcheck: {addr} answered {}", status_line.trim_end());
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("healthcheck: {addr}: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `0.0.0.0` and `::` are listen addresses, not valid destinations. Some network stacks
/// route a connect to them to loopback and others refuse it.
fn loopback_if_unspecified(mut addr: SocketAddr) -> SocketAddr {
    if addr.ip().is_unspecified() {
        addr.set_ip(match addr {
            SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        });
    }
    addr
}

fn probe(addr: SocketAddr) -> std::io::Result<String> {
    let timeout = Duration::from_secs(5);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "GET /api/health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )?;
    let mut status_line = String::new();
    BufReader::new(stream).read_line(&mut status_line)?;
    Ok(status_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthcheck_dials_loopback_for_an_unspecified_bind_address() {
        let dial = |bind: &str| loopback_if_unspecified(bind.parse().unwrap()).to_string();

        assert_eq!(dial("0.0.0.0:8080"), "127.0.0.1:8080");
        assert_eq!(dial("[::]:8080"), "[::1]:8080");
        assert_eq!(dial("192.0.2.7:8080"), "192.0.2.7:8080");
    }
}
