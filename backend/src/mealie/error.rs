use std::error::Error;
use std::io;

use reqwest::StatusCode;

pub(super) const ERROR_BODY_LIMIT: usize = 2048;

/// Every variant has a fixed `Display` text. Mealie localizes its messages and the
/// reqwest and io errors carry their own wording, so none of that may reach a
/// user-facing message. The detail stays in `source()` and `Debug`, for logs.
#[derive(Debug, thiserror::Error)]
pub enum MealieError {
    #[error("Mealie did not respond")]
    Unreachable(#[source] Cause),
    #[error("Mealie did not accept the API key")]
    Unauthorized,
    #[error("Mealie rejected the request")]
    Rejected(RejectedBody),
    #[error("Mealie has no such item")]
    NotFound,
    #[error("Mealie request failed unexpectedly")]
    Other(#[source] Cause),
}

#[derive(Debug, thiserror::Error)]
pub enum Cause {
    #[error("HTTP status {0}")]
    Status(StatusCode),
    #[error(transparent)]
    Transport(reqwest::Error),
    #[error(transparent)]
    File(io::Error),
}

#[derive(Debug)]
pub struct RejectedBody(String);

impl RejectedBody {
    pub fn new(mut body: String) -> Self {
        body.truncate(body.floor_char_boundary(ERROR_BODY_LIMIT));
        Self(body)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) enum Failure {
    Status(StatusCode, String),
    Transport(reqwest::Error),
}

pub(super) fn classify(failure: Failure) -> MealieError {
    match failure {
        Failure::Status(StatusCode::BAD_REQUEST, body) => {
            MealieError::Rejected(RejectedBody::new(body))
        }
        Failure::Status(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN, _) => {
            MealieError::Unauthorized
        }
        Failure::Status(StatusCode::NOT_FOUND, _) => MealieError::NotFound,
        Failure::Status(status, _) if status.is_server_error() => {
            MealieError::Unreachable(Cause::Status(status))
        }
        Failure::Status(status, _) => MealieError::Other(Cause::Status(status)),
        Failure::Transport(error) if lost_contact(&error) => {
            MealieError::Unreachable(Cause::Transport(error))
        }
        Failure::Transport(error) => MealieError::Other(Cause::Transport(error)),
    }
}

/// Walks the source chain because `Response::json` wraps a body that was cut off in a
/// decode error, and that is a lost connection, not bad JSON.
fn lost_contact(error: &reqwest::Error) -> bool {
    let mut link: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(current) = link {
        if let Some(error) = current.downcast_ref::<reqwest::Error>()
            && (error.is_timeout() || error.is_connect() || error.is_request() || error.is_body())
        {
            return true;
        }
        link = current.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_body_is_cut_at_a_character_boundary() {
        let body = format!("a{}", "é".repeat(ERROR_BODY_LIMIT));
        assert!(!body.is_char_boundary(ERROR_BODY_LIMIT));

        let MealieError::Rejected(kept) = classify(Failure::Status(StatusCode::BAD_REQUEST, body))
        else {
            panic!("a 400 is Rejected");
        };

        assert_eq!(kept.as_str().len(), ERROR_BODY_LIMIT - 1);
        assert!(kept.as_str().ends_with('é'));
    }

    #[test]
    fn display_is_fixed_text_that_leaves_the_cause_and_body_out() {
        let rejected = classify(Failure::Status(
            StatusCode::BAD_REQUEST,
            "Os serviços de IA não estão ativados".to_owned(),
        ));
        let server = classify(Failure::Status(StatusCode::BAD_GATEWAY, String::new()));
        let file = MealieError::Other(Cause::File(io::Error::other("disk on fire")));

        assert_eq!(rejected.to_string(), "Mealie rejected the request");
        assert_eq!(server.to_string(), "Mealie did not respond");
        assert_eq!(file.to_string(), "Mealie request failed unexpectedly");
        assert_eq!(
            server.source().unwrap().to_string(),
            "HTTP status 502 Bad Gateway"
        );
        assert_eq!(file.source().unwrap().to_string(), "disk on fire");
    }
}
