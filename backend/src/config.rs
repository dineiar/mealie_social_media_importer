use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use secrecy::SecretString;
use tracing_subscriber::EnvFilter;
use url::Url;

#[derive(Debug)]
pub struct Config {
    pub mealie_url: BaseUrl,
    pub mealie_public_url: BaseUrl,
    pub mealie_timeout: Duration,
    pub mealie_create_tags: bool,
    pub instagram_cookies_file: PathBuf,
    pub transcription: Option<Transcription>,
    pub data_dir: PathBuf,
    pub media_retention: Duration,
    pub max_media_mb: NonZeroU32,
    pub max_concurrent_imports: NonZeroUsize,
    pub tool_timeout: Duration,
    pub tools_auto_update: bool,
    pub bind_addr: SocketAddr,
    pub log_filter: String,
    pub log_format: LogFormat,
}

#[derive(Debug)]
pub struct Transcription {
    pub base_url: BaseUrl,
    pub api_key: Option<SecretString>,
    pub model: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl(Url);

impl BaseUrl {
    fn parse(value: &str) -> Result<Self, String> {
        let url = Url::parse(value)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https"))
            .ok_or("must be an http or https URL")?;
        if url.query().is_some() || url.fragment().is_some() {
            return Err("must not have a query or fragment".to_owned());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("must not contain a username or password".to_owned());
        }
        Ok(Self(url))
    }

    /// Keeps the base's own path, such as `/v1`, which `Url::join` would drop, and
    /// percent-encodes each segment so a `/` inside one can't change the path.
    pub fn join<I>(&self, segments: I) -> Url
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let mut url = self.0.clone();
        url.path_segments_mut()
            .expect("http and https URLs can be a base")
            .pop_if_empty()
            .extend(segments);
        url
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{var} {problem}")]
pub struct ConfigError {
    pub var: &'static str,
    problem: String,
}

impl Config {
    pub fn from_vars(values: &HashMap<String, String>) -> Result<Self, Vec<ConfigError>> {
        let mut vars = Vars::new(values);
        let mealie_url = vars.required("MEALIE_URL", BaseUrl::parse);
        let mealie_public_url = vars.optional("MEALIE_PUBLIC_URL", BaseUrl::parse);
        let mealie_timeout = vars
            .optional("MEALIE_TIMEOUT_SECS", seconds)
            .unwrap_or(Duration::from_secs(330));
        let mealie_create_tags = vars.optional("MEALIE_CREATE_TAGS", boolean).unwrap_or(true);
        let instagram_cookies_file = vars
            .optional("INSTAGRAM_COOKIES_FILE", as_is)
            .unwrap_or_else(|| "/config/cookies/instagram.txt".into());
        let transcription_base_url = vars.optional("TRANSCRIPTION_BASE_URL", BaseUrl::parse);
        let transcription_api_key = vars.optional("TRANSCRIPTION_API_KEY", as_is);
        let transcription_model = vars
            .optional("TRANSCRIPTION_MODEL", as_is)
            .unwrap_or_else(|| "whisper-1".to_owned());
        let transcription_language = vars.optional("TRANSCRIPTION_LANGUAGE", language_code);
        let data_dir = vars
            .optional("DATA_DIR", as_is)
            .unwrap_or_else(|| "/data".into());
        let media_retention = vars
            .optional("MEDIA_RETENTION_MINUTES", minutes)
            .unwrap_or(Duration::from_mins(60));
        let max_media_mb = vars
            .optional("MAX_MEDIA_MB", above_zero)
            .unwrap_or(const { NonZeroU32::new(250).unwrap() });
        let max_concurrent_imports = vars
            .optional("MAX_CONCURRENT_IMPORTS", above_zero)
            .unwrap_or(const { NonZeroUsize::new(2).unwrap() });
        let tool_timeout = vars
            .optional("TOOL_TIMEOUT_SECS", seconds)
            .unwrap_or(Duration::from_secs(180));
        let tools_auto_update = vars.optional("TOOLS_AUTO_UPDATE", boolean).unwrap_or(true);
        let bind_addr = read_bind_addr(&mut vars);
        let log_filter = vars
            .optional("RUST_LOG", log_filter)
            .unwrap_or_else(|| "info".to_owned());
        let log_format = vars
            .optional("LOG_FORMAT", log_format)
            .unwrap_or(LogFormat::Pretty);

        let Some(mealie_url) = mealie_url else {
            return Err(vars.errors);
        };
        vars.finish(Config {
            mealie_public_url: mealie_public_url.unwrap_or_else(|| mealie_url.clone()),
            mealie_url,
            mealie_timeout,
            mealie_create_tags,
            instagram_cookies_file,
            transcription: transcription_base_url.map(|base_url| Transcription {
                base_url,
                api_key: transcription_api_key,
                model: transcription_model,
                language: transcription_language,
            }),
            data_dir,
            media_retention,
            max_media_mb,
            max_concurrent_imports,
            tool_timeout,
            tools_auto_update,
            bind_addr,
            log_filter,
            log_format,
        })
    }
}

pub fn bind_addr(values: &HashMap<String, String>) -> Result<SocketAddr, Vec<ConfigError>> {
    let mut vars = Vars::new(values);
    let addr = read_bind_addr(&mut vars);
    vars.finish(addr)
}

fn read_bind_addr(vars: &mut Vars) -> SocketAddr {
    vars.optional("BIND_ADDR", socket_addr)
        .unwrap_or(SocketAddr::from(([0, 0, 0, 0], 8080)))
}

struct Vars<'a> {
    values: &'a HashMap<String, String>,
    errors: Vec<ConfigError>,
}

impl<'a> Vars<'a> {
    fn new(values: &'a HashMap<String, String>) -> Self {
        Self {
            values,
            errors: Vec::new(),
        }
    }

    fn value(&self, var: &str) -> Option<&'a str> {
        self.values
            .get(var)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    fn optional<T>(
        &mut self,
        var: &'static str,
        parse: impl FnOnce(&str) -> Result<T, String>,
    ) -> Option<T> {
        let value = self.value(var)?;
        parse(value)
            .map_err(|problem| self.errors.push(ConfigError { var, problem }))
            .ok()
    }

    fn required<T>(
        &mut self,
        var: &'static str,
        parse: impl FnOnce(&str) -> Result<T, String>,
    ) -> Option<T> {
        if self.value(var).is_none() {
            self.errors.push(ConfigError {
                var,
                problem: "is required".to_owned(),
            });
        }
        self.optional(var, parse)
    }

    fn finish<T>(self, value: T) -> Result<T, Vec<ConfigError>> {
        if self.errors.is_empty() {
            Ok(value)
        } else {
            Err(self.errors)
        }
    }
}

fn as_is<T: for<'v> From<&'v str>>(value: &str) -> Result<T, String> {
    Ok(value.into())
}

fn above_zero<T: FromStr>(value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| "must be a whole number above zero".to_owned())
}

fn seconds(value: &str) -> Result<Duration, String> {
    above_zero::<NonZeroU32>(value).map(|n| Duration::from_secs(n.get().into()))
}

fn minutes(value: &str) -> Result<Duration, String> {
    above_zero::<NonZeroU32>(value).map(|n| Duration::from_mins(n.get().into()))
}

fn boolean(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("must be true or false".to_owned()),
    }
}

fn socket_addr(value: &str) -> Result<SocketAddr, String> {
    value
        .parse()
        .map_err(|_| "must be an IP address and port, such as 0.0.0.0:8080".to_owned())
}

fn language_code(value: &str) -> Result<String, String> {
    if value.len() == 2 && value.bytes().all(|b| b.is_ascii_alphabetic()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err("must be a two-letter ISO 639-1 code, such as pt".to_owned())
    }
}

fn log_filter(value: &str) -> Result<String, String> {
    EnvFilter::try_new(value)
        .map(|_| value.to_owned())
        .map_err(|error| format!("is not a valid log filter: {error}"))
}

fn log_format(value: &str) -> Result<LogFormat, String> {
    match value.to_ascii_lowercase().as_str() {
        "pretty" => Ok(LogFormat::Pretty),
        "json" => Ok(LogFormat::Json),
        _ => Err("must be pretty or json".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    fn valid(pairs: &[(&str, &str)]) -> Config {
        Config::from_vars(&vars(pairs)).expect("configuration is valid")
    }

    fn invalid(pairs: &[(&str, &str)]) -> Vec<&'static str> {
        Config::from_vars(&vars(pairs))
            .expect_err("configuration is invalid")
            .iter()
            .map(|error| error.var)
            .collect()
    }

    #[test]
    fn only_mealie_url_gives_every_default() {
        let config = valid(&[("MEALIE_URL", "http://mealie:9000")]);

        assert_eq!(config.mealie_url.to_string(), "http://mealie:9000/");
        assert_eq!(config.mealie_public_url, config.mealie_url);
        assert_eq!(config.mealie_timeout, Duration::from_secs(330));
        assert!(config.mealie_create_tags);
        assert_eq!(
            config.instagram_cookies_file,
            PathBuf::from("/config/cookies/instagram.txt")
        );
        assert!(config.transcription.is_none());
        assert_eq!(config.data_dir, PathBuf::from("/data"));
        assert_eq!(config.media_retention, Duration::from_secs(60 * 60));
        assert_eq!(config.max_media_mb.get(), 250);
        assert_eq!(config.max_concurrent_imports.get(), 2);
        assert_eq!(config.tool_timeout, Duration::from_secs(180));
        assert!(config.tools_auto_update);
        assert_eq!(config.bind_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.log_filter, "info");
        assert_eq!(config.log_format, LogFormat::Pretty);
    }

    #[test]
    fn transcription_defaults_apply_once_base_url_is_set() {
        let config = valid(&[
            ("MEALIE_URL", "http://mealie:9000"),
            ("TRANSCRIPTION_BASE_URL", "http://whisper:8000/v1"),
        ]);

        let transcription = config.transcription.expect("transcription is on");
        assert!(transcription.api_key.is_none());
        assert_eq!(transcription.model, "whisper-1");
        assert!(transcription.language.is_none());
    }

    #[test]
    fn every_variable_reads_its_value() {
        let config = valid(&[
            ("MEALIE_URL", "http://mealie:9000"),
            ("MEALIE_PUBLIC_URL", "https://recipes.example.com"),
            ("MEALIE_TIMEOUT_SECS", "600"),
            ("MEALIE_CREATE_TAGS", "false"),
            ("INSTAGRAM_COOKIES_FILE", "/secrets/ig.txt"),
            (
                "TRANSCRIPTION_BASE_URL",
                "https://api.example.com/openai/v1",
            ),
            ("TRANSCRIPTION_API_KEY", "0012345"),
            ("TRANSCRIPTION_MODEL", "large-v3"),
            ("TRANSCRIPTION_LANGUAGE", "PT"),
            ("DATA_DIR", "/srv/importer"),
            ("MEDIA_RETENTION_MINUTES", "15"),
            ("MAX_MEDIA_MB", "100"),
            ("MAX_CONCURRENT_IMPORTS", "4"),
            ("TOOL_TIMEOUT_SECS", "30"),
            ("TOOLS_AUTO_UPDATE", "FALSE"),
            ("BIND_ADDR", "[::1]:9090"),
            ("RUST_LOG", "mealie_importer=debug,tower_http=warn"),
            ("LOG_FORMAT", "json"),
        ]);

        assert_eq!(
            config.mealie_public_url.to_string(),
            "https://recipes.example.com/"
        );
        assert_eq!(config.mealie_timeout, Duration::from_secs(600));
        assert!(!config.mealie_create_tags);
        assert_eq!(
            config.instagram_cookies_file,
            PathBuf::from("/secrets/ig.txt")
        );
        let transcription = config.transcription.expect("transcription is on");
        assert_eq!(
            transcription.base_url.to_string(),
            "https://api.example.com/openai/v1"
        );
        assert_eq!(
            transcription
                .api_key
                .as_ref()
                .map(|key| key.expose_secret()),
            Some("0012345")
        );
        assert_eq!(transcription.model, "large-v3");
        assert_eq!(transcription.language.as_deref(), Some("pt"));
        assert_eq!(config.data_dir, PathBuf::from("/srv/importer"));
        assert_eq!(config.media_retention, Duration::from_secs(15 * 60));
        assert_eq!(config.max_media_mb.get(), 100);
        assert_eq!(config.max_concurrent_imports.get(), 4);
        assert_eq!(config.tool_timeout, Duration::from_secs(30));
        assert!(!config.tools_auto_update);
        assert_eq!(config.bind_addr, "[::1]:9090".parse().unwrap());
        assert_eq!(config.log_filter, "mealie_importer=debug,tower_http=warn");
        assert_eq!(config.log_format, LogFormat::Json);
    }

    #[test]
    fn empty_values_count_as_unset() {
        let names = [
            "MEALIE_URL",
            "MEALIE_PUBLIC_URL",
            "MEALIE_TIMEOUT_SECS",
            "TRANSCRIPTION_BASE_URL",
            "MAX_CONCURRENT_IMPORTS",
            "BIND_ADDR",
            "LOG_FORMAT",
        ];
        let mut pairs: Vec<(&str, &str)> = names.iter().map(|name| (*name, "")).collect();

        assert_eq!(invalid(&pairs), ["MEALIE_URL"]);

        pairs[0] = ("MEALIE_URL", "http://mealie:9000");
        let config = valid(&pairs);
        assert_eq!(config.mealie_public_url, config.mealie_url);
        assert_eq!(config.mealie_timeout, Duration::from_secs(330));
        assert!(config.transcription.is_none());
        assert_eq!(config.max_concurrent_imports.get(), 2);
        assert_eq!(config.bind_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.log_format, LogFormat::Pretty);
    }

    #[test]
    fn each_invalid_variable_gives_one_error_in_table_order() {
        let errors = invalid(&[
            ("MEALIE_URL", "mealie:9000"),
            ("MEALIE_PUBLIC_URL", "ftp://recipes.example.com"),
            ("MEALIE_TIMEOUT_SECS", "0"),
            ("MEALIE_CREATE_TAGS", "yes"),
            ("TRANSCRIPTION_BASE_URL", "http://whisper:8000/v1?model=x"),
            ("TRANSCRIPTION_LANGUAGE", "pt-BR"),
            ("MEDIA_RETENTION_MINUTES", "-5"),
            ("MAX_MEDIA_MB", "lots"),
            ("MAX_CONCURRENT_IMPORTS", "0"),
            ("TOOL_TIMEOUT_SECS", "1.5"),
            ("TOOLS_AUTO_UPDATE", "1"),
            ("BIND_ADDR", "localhost:8080"),
            ("RUST_LOG", "mealie_importer=loud"),
            ("LOG_FORMAT", "xml"),
        ]);

        assert_eq!(
            errors,
            [
                "MEALIE_URL",
                "MEALIE_PUBLIC_URL",
                "MEALIE_TIMEOUT_SECS",
                "MEALIE_CREATE_TAGS",
                "TRANSCRIPTION_BASE_URL",
                "TRANSCRIPTION_LANGUAGE",
                "MEDIA_RETENTION_MINUTES",
                "MAX_MEDIA_MB",
                "MAX_CONCURRENT_IMPORTS",
                "TOOL_TIMEOUT_SECS",
                "TOOLS_AUTO_UPDATE",
                "BIND_ADDR",
                "RUST_LOG",
                "LOG_FORMAT",
            ]
        );
    }

    #[test]
    fn a_url_with_credentials_is_rejected_without_echoing_them() {
        for name in ["MEALIE_URL", "MEALIE_PUBLIC_URL", "TRANSCRIPTION_BASE_URL"] {
            for value in [
                "http://user:s3cret@mealie:9000",
                "http://s3cret@mealie:9000",
            ] {
                let mut values = vars(&[("MEALIE_URL", "http://mealie:9000")]);
                values.insert(name.to_owned(), value.to_owned());
                let errors = Config::from_vars(&values).expect_err("credentials are refused");

                let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
                assert_eq!(lines.len(), 1, "{name}");
                assert!(lines[0].starts_with(name), "{}", lines[0]);
                assert!(!lines[0].contains("s3cret"), "{}", lines[0]);
            }
        }
    }

    #[test]
    fn missing_mealie_url_is_the_only_required_variable() {
        assert_eq!(invalid(&[]), ["MEALIE_URL"]);
    }

    #[test]
    fn base_url_join_keeps_the_base_path() {
        let join = |base: &str, segments: &[&str]| {
            BaseUrl::parse(base).unwrap().join(segments).to_string()
        };

        assert_eq!(
            join("http://proxy/mealie/", &["api", "users", "self"]),
            "http://proxy/mealie/api/users/self"
        );
        assert_eq!(
            join("http://whisper:8000/v1", &["audio", "transcriptions"]),
            "http://whisper:8000/v1/audio/transcriptions"
        );
        assert_eq!(
            join("http://mealie:9000", &["api", "recipes", "a/b"]),
            "http://mealie:9000/api/recipes/a%2Fb"
        );
    }

    #[test]
    fn bind_addr_ignores_every_other_variable() {
        let others = [("MEALIE_TIMEOUT_SECS", "0"), ("LOG_FORMAT", "xml")];

        assert_eq!(
            bind_addr(&vars(&others)).unwrap(),
            "0.0.0.0:8080".parse().unwrap()
        );
        assert_eq!(
            bind_addr(&vars(&[
                ("BIND_ADDR", "127.0.0.1:9000"),
                others[0],
                others[1]
            ]))
            .unwrap(),
            "127.0.0.1:9000".parse().unwrap()
        );
        let errors = bind_addr(&vars(&[("BIND_ADDR", "nope")])).unwrap_err();
        assert_eq!(
            errors.iter().map(|e| e.var).collect::<Vec<_>>(),
            ["BIND_ADDR"]
        );
    }
}
