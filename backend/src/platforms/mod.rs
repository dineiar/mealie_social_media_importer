mod cookies;
mod instagram;
mod registry;

use async_trait::async_trait;
use serde::Serialize;
use ts_rs::TS;
use url::Url;

pub use cookies::{CheckIn, Checkout, CookieError, CookieStore, SessionCookie};
pub use instagram::Instagram;
pub use registry::{Registry, Resolved};

#[async_trait]
pub trait Platform: Send + Sync {
    fn id(&self) -> &'static str;

    fn name(&self) -> &'static str;

    /// Whether the URL's scheme, host, port and userinfo all belong to this platform.
    fn matches(&self, url: &Url) -> bool;

    /// Rebuilds the link from validated parts. The query string and fragment never survive,
    /// so the downloader only ever sees a URL this platform composed.
    fn canonicalize(&self, url: &Url) -> Result<PostLink, Rejected>;

    async fn status(&self) -> PlatformStatus;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostLink {
    /// The URL names the post. The downloader still builds the final canonical URL from the
    /// shortcode it reads back, because the path's reel or post segment isn't always right.
    Post(Url),
    /// A share token isn't a shortcode. Only the download reveals the post, and its canonical
    /// URL replaces this one on the import and as the recipe's `orgURL`.
    Share(Url),
}

impl PostLink {
    pub fn url(&self) -> &Url {
        match self {
            Self::Post(url) | Self::Share(url) => url,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    /// No platform claims the URL, whether it is another site, a look-alike, a URL with
    /// userinfo or a non-default port, or no URL at all.
    UnsupportedHost,
    NotAPost,
}

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
