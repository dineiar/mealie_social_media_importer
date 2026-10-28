mod cookies;

use serde::Serialize;
use ts_rs::TS;

pub use cookies::{CheckIn, Checkout, CookieError, CookieStore, SessionCookie};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformStatus {
    Ready,
    NotConfigured(SetupHint),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SetupHint {
    CookiesMissing,
    CookiesUnavailable,
    CookiesInvalid,
    CookiesNoSession,
}
