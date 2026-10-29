use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::wire::FileKind;

const MAX_LEN: usize = 128;

struct FileType {
    extension: &'static str,
    kind: FileKind,
    mime: &'static str,
}

const TYPES: [FileType; 6] = [
    FileType::new("mp4", FileKind::Video, "video/mp4"),
    FileType::new("jpg", FileKind::Image, "image/jpeg"),
    FileType::new("jpeg", FileKind::Image, "image/jpeg"),
    FileType::new("png", FileKind::Image, "image/png"),
    FileType::new("webp", FileKind::Image, "image/webp"),
    FileType::new("txt", FileKind::Text, "text/plain"),
];

impl FileType {
    const fn new(extension: &'static str, kind: FileKind, mime: &'static str) -> Self {
        Self {
            extension,
            kind,
            mime,
        }
    }

    fn of(name: &str) -> Option<&'static Self> {
        let (_, extension) = name.rsplit_once('.')?;
        TYPES.iter().find(|known| known.extension == extension)
    }
}

/// A downloadable file in an import's directory. It is one path segment of ASCII letters,
/// digits, `.`, `-` and `_` that starts with a letter or digit, so it can't climb out of
/// the directory or name `manifest.json`, and it needs no escaping in a URL or a header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct FileName(String);

#[derive(Debug, thiserror::Error)]
#[error("not a downloadable file name")]
pub struct InvalidFileName;

impl FileName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn kind(&self) -> FileKind {
        self.file_type().kind
    }

    pub fn mime(&self) -> &'static str {
        self.file_type().mime
    }

    fn file_type(&self) -> &'static FileType {
        FileType::of(&self.0).expect("a parsed name has a known extension")
    }
}

impl FromStr for FileName {
    type Err = InvalidFileName;

    fn from_str(name: &str) -> Result<Self, InvalidFileName> {
        let well_formed = name.len() <= MAX_LEN
            && name.starts_with(|c: char| c.is_ascii_alphanumeric())
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
        if well_formed && FileType::of(name).is_some() {
            Ok(Self(name.to_owned()))
        } else {
            Err(InvalidFileName)
        }
    }
}

impl TryFrom<String> for FileName {
    type Error = InvalidFileName;

    fn try_from(name: String) -> Result<Self, InvalidFileName> {
        name.parse()
    }
}

impl From<FileName> for String {
    fn from(name: FileName) -> Self {
        name.0
    }
}

impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_and_text_names_parse_with_their_kind_and_mime() {
        let parsed = |name: &str| {
            let name: FileName = name.parse().expect("a downloadable name");
            (name.kind(), name.mime())
        };

        assert_eq!(parsed("video-1.mp4"), (FileKind::Video, "video/mp4"));
        assert_eq!(parsed("cover.jpg"), (FileKind::Image, "image/jpeg"));
        assert_eq!(parsed("photo_2.webp"), (FileKind::Image, "image/webp"));
        assert_eq!(parsed("caption.txt"), (FileKind::Text, "text/plain"));
    }

    #[test]
    fn names_that_could_leave_the_directory_or_are_not_media_are_refused() {
        for name in [
            "",
            ".",
            "..",
            "../manifest.json",
            "..\\caption.txt",
            "a/b.mp4",
            "/etc/passwd.txt",
            ".hidden.txt",
            "-rf.txt",
            "manifest.json",
            "manifest.json.tmp",
            "video.mp4\0.txt",
            "vídeo.mp4",
            "caption.TXT",
            "caption",
            "audio.mp3",
            "café%2F.txt",
            &format!("{}.txt", "a".repeat(MAX_LEN)),
        ] {
            assert!(name.parse::<FileName>().is_err(), "{name:?} was accepted");
        }
    }
}
