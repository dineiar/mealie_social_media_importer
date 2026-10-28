use std::path::{Path, PathBuf};

use async_trait::async_trait;
use url::Url;

use super::{CookieStore, Platform, PlatformStatus, PostLink, Rejected, SessionCookie, SetupHint};

const ID: &str = "instagram";

const HOSTS: [&str; 3] = ["instagram.com", "www.instagram.com", "instagr.am"];

/// gallery-dl matches `instagram.com` and `www.instagram.com` but rejects `instagr.am`, so
/// every canonical URL uses this host.
const CANONICAL_HOST: &str = "www.instagram.com";

const SESSION: SessionCookie = SessionCookie {
    domain: ".instagram.com",
    name: "sessionid",
};

const POST_KINDS: [&str; 4] = ["p", "reel", "reels", "tv"];

pub struct Instagram {
    cookies: CookieStore,
}

impl Instagram {
    pub fn new(cookies_dir: &Path, original: PathBuf) -> Self {
        Self {
            cookies: CookieStore::new(cookies_dir, ID, original, SESSION),
        }
    }
}

#[async_trait]
impl Platform for Instagram {
    fn id(&self) -> &'static str {
        ID
    }

    fn name(&self) -> &'static str {
        "Instagram"
    }

    fn matches(&self, url: &Url) -> bool {
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.host_str().is_some_and(|host| HOSTS.contains(&host))
    }

    fn canonicalize(&self, url: &Url) -> Result<PostLink, Rejected> {
        if !self.matches(url) {
            return Err(Rejected::UnsupportedHost);
        }
        let mut segments: Vec<&str> = url.path_segments().ok_or(Rejected::NotAPost)?.collect();
        if segments.last() == Some(&"") {
            segments.pop();
        }
        match segments[..] {
            [kind, code] if POST_KINDS.contains(&kind) && is_code(code) => {
                Ok(PostLink::Post(canonical(&[singular(kind), code])))
            }
            ["share", token] if is_code(token) && !POST_KINDS.contains(&token) => {
                Ok(PostLink::Share(canonical(&["share", token])))
            }
            ["share", kind, token] if POST_KINDS.contains(&kind) && is_code(token) => {
                Ok(PostLink::Share(canonical(&[
                    "share",
                    singular(kind),
                    token,
                ])))
            }
            _ => Err(Rejected::NotAPost),
        }
    }

    async fn status(&self) -> PlatformStatus {
        self.cookies.status().await.unwrap_or_else(|error| {
            tracing::error!(%error, "cannot check the Instagram cookies");
            PlatformStatus::NotConfigured(SetupHint::CookiesUnavailable)
        })
    }
}

fn singular(kind: &str) -> &str {
    if kind == "reels" { "reel" } else { kind }
}

fn is_code(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn canonical(segments: &[&str]) -> Url {
    let path = segments.join("/");
    Url::parse(&format!("https://{CANONICAL_HOST}/{path}/")).expect("validated parts form a URL")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::TempDir;

    use crate::platforms::Registry;

    use super::*;

    fn instagram() -> Instagram {
        Instagram::new(Path::new("/unused"), PathBuf::from("/unused.txt"))
    }

    fn resolve(input: &str) -> Result<PostLink, Rejected> {
        let registry = Registry::new(vec![Arc::new(instagram())]);
        registry.resolve(input).map(|resolved| resolved.link)
    }

    fn canonical(path: &str) -> Url {
        Url::parse(&format!("https://www.instagram.com/{path}/")).unwrap()
    }

    fn post(path: &str) -> PostLink {
        PostLink::Post(canonical(path))
    }

    fn share(path: &str) -> PostLink {
        PostLink::Share(canonical(path))
    }

    fn assert_rejected(expected: Rejected, inputs: &[&str]) {
        for input in inputs {
            assert_eq!(resolve(input), Err(expected), "{input:?}");
        }
    }

    #[test]
    fn every_accepted_form_becomes_a_canonical_link_without_query_or_fragment() {
        let cases = [
            (
                "https://www.instagram.com/p/C9xYz12AbCd/",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/reel/C9xYz12AbCd/",
                post("reel/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/reels/C9xYz12AbCd/",
                post("reel/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/tv/C9xYz12AbCd/",
                post("tv/C9xYz12AbCd"),
            ),
            (
                "https://instagram.com/p/C9xYz12AbCd/",
                post("p/C9xYz12AbCd"),
            ),
            ("https://instagr.am/p/C9xYz12AbCd/", post("p/C9xYz12AbCd")),
            (
                "https://instagr.am/reel/C9xYz12AbCd/",
                post("reel/C9xYz12AbCd"),
            ),
            (
                "http://www.instagram.com/p/C9xYz12AbCd/",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://WWW.Instagram.COM/p/C9xYz12AbCd/",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com:443/p/C9xYz12AbCd/",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/p/C9xYz12AbCd",
                post("p/C9xYz12AbCd"),
            ),
            (
                "  https://www.instagram.com/p/C9xYz12AbCd/\n",
                post("p/C9xYz12AbCd"),
            ),
            ("https://www.instagram.com/p/a_b-C/", post("p/a_b-C")),
            (
                "https://www.instagram.com/p/C9xYz12AbCd/?igsh=MWRsbW5qZHVybGJkdg==",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/reel/C9xYz12AbCd/?utm_source=ig_web_copy_link&utm_medium=copy_link&igsh=abc",
                post("reel/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/p/C9xYz12AbCd/?img_index=2",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/p/C9xYz12AbCd/?",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/p/C9xYz12AbCd/#comments",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/p/C9xYz12AbCd/?igsh=abc#comments",
                post("p/C9xYz12AbCd"),
            ),
            (
                "https://www.instagram.com/share/BAhcdEfgh/",
                share("share/BAhcdEfgh"),
            ),
            (
                "https://www.instagram.com/share/p/BAhcdEfgh/",
                share("share/p/BAhcdEfgh"),
            ),
            (
                "https://www.instagram.com/share/reel/BAhcdEfgh/",
                share("share/reel/BAhcdEfgh"),
            ),
            (
                "https://www.instagram.com/share/tv/BAhcdEfgh",
                share("share/tv/BAhcdEfgh"),
            ),
            (
                "https://www.instagram.com/share/reels/BAhcdEfgh/",
                share("share/reel/BAhcdEfgh"),
            ),
            (
                "https://instagram.com/share/reel/BAhcdEfgh/?igsh=abc&utm_source=qr",
                share("share/reel/BAhcdEfgh"),
            ),
            (
                "https://instagr.am/share/BAhcdEfgh",
                share("share/BAhcdEfgh"),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(resolve(input), Ok(expected), "{input}");
        }
    }

    #[test]
    fn other_hosts_and_look_alikes_reach_no_platform() {
        assert_rejected(
            Rejected::UnsupportedHost,
            &[
                "https://example.com/p/C9xYz12AbCd/",
                "https://www.tiktok.com/@someone/video/7300000000000000000",
                "https://youtube.com/watch?v=dQw4w9WgXcQ",
                "https://instagram.com.evil.test/p/C9xYz12AbCd/",
                "https://www.instagram.com.evil.test/p/C9xYz12AbCd/",
                "https://evilinstagram.com/p/C9xYz12AbCd/",
                "https://notinstagram.com/p/C9xYz12AbCd/",
                "https://instagram.com.evil.test@www.instagram.com.evil.test/p/C9xYz12AbCd/",
                "https://instagram.com@evil.test/p/C9xYz12AbCd/",
                "https://www.instagram.com@evil.test/p/C9xYz12AbCd/",
                "https://www.instagram.com:443@evil.test/p/C9xYz12AbCd/",
                "https://www.instagram.com%2eevil.test/p/C9xYz12AbCd/",
                "https://evil.test\\@www.instagram.com/p/C9xYz12AbCd/",
                "https://evil.test/https://www.instagram.com/p/C9xYz12AbCd/",
                "https://evil.test/?next=https://www.instagram.com/p/C9xYz12AbCd/",
                "https://evil.test/#@www.instagram.com/p/C9xYz12AbCd/",
                "https://www.instagram.com./p/C9xYz12AbCd/",
                "https://www.instagram.co/p/C9xYz12AbCd/",
                "https://www.instagram.com.au/p/C9xYz12AbCd/",
                "https://xn--instagram-9ve.com/p/C9xYz12AbCd/",
                "https://www.instаgram.com/p/C9xYz12AbCd/",
                "https://www.instagr.am/p/C9xYz12AbCd/",
                "https://m.instagram.com/p/C9xYz12AbCd/",
                "https://l.instagram.com/?u=https%3A%2F%2Fevil.test%2F",
                "https://ddinstagram.com/p/C9xYz12AbCd/",
                "https://127.0.0.1/p/C9xYz12AbCd/",
                "https://[::1]/p/C9xYz12AbCd/",
                "https://2130706433/p/C9xYz12AbCd/",
            ],
        );
    }

    #[test]
    fn userinfo_ports_and_schemes_on_the_right_host_are_rejected() {
        assert_rejected(
            Rejected::UnsupportedHost,
            &[
                "https://user@www.instagram.com/p/C9xYz12AbCd/",
                "https://user:password@www.instagram.com/p/C9xYz12AbCd/",
                "https://:password@www.instagram.com/p/C9xYz12AbCd/",
                "https://www.instagram.com:8443/p/C9xYz12AbCd/",
                "https://www.instagram.com:80/p/C9xYz12AbCd/",
                "http://www.instagram.com:443/p/C9xYz12AbCd/",
                "https://www.instagram.com:0/p/C9xYz12AbCd/",
                "ftp://www.instagram.com/p/C9xYz12AbCd/",
                "file://www.instagram.com/p/C9xYz12AbCd/",
                "javascript://www.instagram.com/p/C9xYz12AbCd/",
                "data:text/plain,https://www.instagram.com/p/C9xYz12AbCd/",
            ],
        );
    }

    #[test]
    fn text_that_is_not_an_absolute_url_reaches_no_platform() {
        assert_rejected(
            Rejected::UnsupportedHost,
            &[
                "",
                "   ",
                "not a url",
                "www.instagram.com/p/C9xYz12AbCd/",
                "instagram.com/p/C9xYz12AbCd/",
                "//www.instagram.com/p/C9xYz12AbCd/",
                "/p/C9xYz12AbCd/",
                "https://",
                "https:///p/C9xYz12AbCd/",
            ],
        );
    }

    #[test]
    fn an_instagram_path_that_is_not_one_post_is_rejected() {
        assert_rejected(
            Rejected::NotAPost,
            &[
                "https://www.instagram.com/",
                "https://www.instagram.com/reels/",
                "https://www.instagram.com/reels",
                "https://www.instagram.com/reel/",
                "https://www.instagram.com/p/",
                "https://www.instagram.com/tv/",
                "https://www.instagram.com/p//C9xYz12AbCd/",
                "https://www.instagram.com/p/C9xYz12AbCd//",
                "https://www.instagram.com/p/C9xYz12AbCd/embed/",
                "https://www.instagram.com/p/C9xYz12AbCd/c/17900000000000000/",
                "https://www.instagram.com/reels/audio/1234567890/",
                "https://www.instagram.com/reels/C9xYz12AbCd/extra/",
                "https://www.instagram.com/explore/",
                "https://www.instagram.com/explore/tags/pasta/",
                "https://www.instagram.com/mara/",
                "https://www.instagram.com/mara/reels/",
                "https://www.instagram.com/stories/mara/3000000000000000000/",
                "https://www.instagram.com/accounts/login/?next=/p/C9xYz12AbCd/",
                "https://www.instagram.com/mara/reel/C9xYz12AbCd/",
                "https://www.instagram.com/mara/p/C9xYz12AbCd/",
                "https://www.instagram.com/P/C9xYz12AbCd/",
                "https://www.instagram.com/Reel/C9xYz12AbCd/",
                "https://www.instagram.com/p/C9x%2FYz12/",
                "https://www.instagram.com/p/C9x%20Yz12/",
                "https://www.instagram.com/p/C9x.Yz12/",
                "https://www.instagram.com/p/C9x;Yz12/",
                "https://www.instagram.com/p/%E2%80%AE/",
                "https://www.instagram.com/share/",
                "https://www.instagram.com/share//",
                "https://www.instagram.com/share/reel/",
                "https://www.instagram.com/share/p",
                "https://www.instagram.com/share/stories/BAhcdEfgh/",
                "https://www.instagram.com/share/reel/BAhcdEfgh/extra/",
                "https://www.instagram.com/share/reel//BAhcdEfgh/",
                "https://www.instagram.com/share/BAhc.dEfgh/",
                "https://www.instagram.com/p/C9xYz12AbCd/../../mara/",
            ],
        );
        assert_eq!(
            resolve("https://www.instagram.com/explore/../p/C9xYz12AbCd/"),
            Ok(post("p/C9xYz12AbCd"))
        );
    }

    #[test]
    fn canonicalize_refuses_a_url_the_platform_does_not_match() {
        let url = Url::parse("https://instagram.com.evil.test/p/C9xYz12AbCd/").unwrap();

        assert!(!instagram().matches(&url));
        assert_eq!(
            instagram().canonicalize(&url),
            Err(Rejected::UnsupportedHost)
        );
    }

    #[tokio::test]
    async fn a_cookies_file_the_backend_cannot_read_is_reported_as_unavailable() {
        let dir = TempDir::new().unwrap();
        let original = dir.path().join("instagram.txt");
        fs::create_dir(&original).unwrap();

        let status = Instagram::new(&dir.path().join("data"), original)
            .status()
            .await;

        assert_eq!(
            status,
            PlatformStatus::NotConfigured(SetupHint::CookiesUnavailable)
        );
    }
}
