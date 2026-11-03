//! Wire delivery capabilities, independent of language renderer features.
//! These describe API shapes, not model-specific modality support.
#[cfg(any(
    test,
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "s3",
    feature = "images",
    feature = "transcription"
))]
use crate::{Error, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AssetDialect {
    Claude,
    Openai,
    OpenaiChat,
    Gemini,
    OpenaiEmbedding,
    GeminiEmbedding,
    OpenaiImages,
    GeminiImages,
    OpenaiTranscription,
    GeminiTranscription,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum MediaKind {
    Image,
    Document,
    Audio,
    Video,
    File,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DeliveryMethod {
    Inline,
    Url,
}
/// Which delivery a client asks for when a target accepts both. `Inline` needs
/// no object store; `PreferUrl` keeps long histories from resending large files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DeliveryMode {
    #[default]
    Inline,
    PreferUrl,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DeliveryConstraints {
    /// Empty/non-exhaustive lists leave unlisted formats to the upstream API.
    #[cfg_attr(feature = "ts", ts(as = "Vec<String>"))]
    pub media_types: &'static [&'static str],
    pub exhaustive: bool,
    /// Raw asset bytes, not base64 length. None means no documented limit here.
    pub max_bytes: Option<u64>,
    pub exclusive: bool,
}
#[cfg(any(
    test,
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "s3",
    feature = "images",
    feature = "transcription"
))]
impl DeliveryConstraints {
    pub(crate) fn accepts_mime(&self, mime: Option<&str>) -> bool {
        !self.exhaustive || mime.is_none_or(|mime| self.media_types.contains(&mime))
    }
    pub(crate) fn check(&self, mime: Option<&str>, bytes: Option<u64>) -> Result<()> {
        if !self.accepts_mime(mime) {
            return Err(Error::invalid(
                "media format is unsupported for this delivery method; no transcoding is performed",
            ));
        }
        if let (Some(size), Some(limit)) = (bytes, self.max_bytes)
            && (size > limit || (self.exclusive && size == limit))
        {
            return Err(Error::invalid("asset exceeds the documented byte limit"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MediaCapability {
    pub inline: Option<DeliveryConstraints>,
    pub url: Option<DeliveryConstraints>,
    /// Complete serialized native request, including text and base64 overhead.
    pub max_request_bytes: Option<u64>,
}
impl MediaCapability {
    /// The mode's method when the target accepts it, otherwise the other one.
    pub fn preferred(&self, mode: DeliveryMode) -> Option<DeliveryMethod> {
        let url = self.url.map(|_| DeliveryMethod::Url);
        let inline = self.inline.map(|_| DeliveryMethod::Inline);
        match mode {
            DeliveryMode::Inline => inline.or(url),
            DeliveryMode::PreferUrl => url.or(inline),
        }
    }
    #[cfg(any(
        test,
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini",
        feature = "embeddings",
        feature = "s3",
        feature = "images",
        feature = "transcription"
    ))]
    pub(crate) fn for_mime(mut self, mime: Option<&str>) -> Self {
        self.inline = self.inline.filter(|c| c.accepts_mime(mime));
        self.url = self.url.filter(|c| c.accepts_mime(mime));
        self
    }
}
// upstream_docs/claude/docs/API overview.md:111: complete direct API body, 32 MB.
pub(crate) const CLAUDE_MAX_REQUEST_BYTES: u64 = 32_000_000;
const ANY: DeliveryConstraints = DeliveryConstraints {
    media_types: &[],
    exhaustive: false,
    max_bytes: None,
    exclusive: false,
};
const CLAUDE_IMAGE: DeliveryConstraints = DeliveryConstraints {
    // upstream_docs/claude/docs/Create a Message.md:382-390; content::ImageMediaType.
    media_types: &["image/jpeg", "image/png", "image/gif", "image/webp"],
    exhaustive: true,
    ..ANY
};
const CLAUDE_DOCUMENT: DeliveryConstraints = DeliveryConstraints {
    // upstream_docs/claude/docs/Create a Message.md:434-444; DocumentSource Base64/Text.
    media_types: &["application/pdf", "text/plain"],
    exhaustive: true,
    ..ANY
};
const PDF: DeliveryConstraints = DeliveryConstraints {
    media_types: &["application/pdf"],
    exhaustive: true,
    ..ANY
};
const CHAT_AUDIO: DeliveryConstraints = DeliveryConstraints {
    // upstream_docs/openai/docs/Create chat completion.md:228-236; InputAudio.format.
    media_types: &["audio/wav", "audio/x-wav", "audio/mpeg", "audio/mp3"],
    exhaustive: true,
    ..ANY
};
const GEMINI_FILE: DeliveryConstraints = DeliveryConstraints {
    // upstream_docs/gemini/docs/Changelog.md:522-523: Gemini API file input 100 MB.
    // MB uses SI bytes. This is a per-file cap, not a total JSON request cap.
    max_bytes: Some(100_000_000),
    ..ANY
};
// upstream_docs/gemini/docs/Generating content.md:3513 lists examples, not a
// complete whitelist. Preserve that distinction for caller-side format choices.
const GEMINI_IMAGE: DeliveryConstraints = DeliveryConstraints {
    media_types: &[
        "image/png",
        "image/jpeg",
        "image/jpg",
        "image/webp",
        "image/heic",
        "image/heif",
        "image/gif",
        "image/avif",
    ],
    ..GEMINI_FILE
};
const GEMINI_AUDIO: DeliveryConstraints = DeliveryConstraints {
    media_types: &["audio/*", "video/audio/s16le", "video/audio/wav"],
    ..GEMINI_FILE
};
const GEMINI_DOCUMENT: DeliveryConstraints = DeliveryConstraints {
    media_types: &["application/pdf", "text/plain"],
    ..GEMINI_FILE
};
const GEMINI_ATTACHMENT: DeliveryConstraints = DeliveryConstraints {
    media_types: &[
        "text/html",
        "text/css",
        "text/javascript",
        "text/x-typescript",
        "text/csv",
        "text/markdown",
        "text/x-python",
        "text/xml",
        "text/rtf",
        "application/json",
        "application/x-ipynb+json",
        "application/rtf",
    ],
    ..GEMINI_FILE
};
const GEMINI_VIDEO: DeliveryConstraints = DeliveryConstraints {
    // upstream_docs/gemini/docs/Video understanding.md:421-433 (formats).
    media_types: &[
        "video/mp4",
        "video/mpeg",
        "video/mov",
        "video/avi",
        "video/x-flv",
        "video/mpg",
        "video/webm",
        "video/wmv",
        "video/3gpp",
    ],
    exhaustive: true,
    ..GEMINI_FILE
};
/// Base table for a message part. Tool-result envelopes can narrow it further.
/// OpenAI image/file MIME restrictions and per-asset byte caps are not specified
/// in the checked-in endpoint references, so they are deliberately left open.
pub fn media_capability(target: AssetDialect, kind: MediaKind) -> MediaCapability {
    use AssetDialect as D;
    use MediaKind as K;
    let (inline, url) = match (target, kind) {
        // Create image edit.md:11-25: JSON URL or base64 data URL image references.
        // The 20971520-character URI limit is checked after encoding, not guessed as raw bytes.
        (D::OpenaiImages, K::Image) => (Some(ANY), Some(ANY)),
        (D::GeminiImages, K::Image) => (Some(GEMINI_IMAGE), Some(GEMINI_IMAGE)),
        // Create transcription.md:17 documents these upload container formats.
        // No per-file byte cap is stated in that checked-in endpoint reference.
        (D::OpenaiTranscription, K::Audio) => (
            Some(DeliveryConstraints {
                media_types: &[
                    "audio/flac",
                    "audio/mp3",
                    "audio/mpeg",
                    "audio/mpga",
                    "audio/mp4",
                    "video/mp4",
                    "audio/m4a",
                    "audio/ogg",
                    "audio/wav",
                    "audio/x-wav",
                    "audio/webm",
                    "video/webm",
                ],
                exhaustive: true,
                ..ANY
            }),
            None,
        ),
        (D::GeminiTranscription, K::Audio) => (Some(GEMINI_AUDIO), Some(GEMINI_AUDIO)),
        // Message.md:372-400,424-475; ImageSource/DocumentSource in protocol.
        (D::Claude, K::Image) => (Some(CLAUDE_IMAGE), Some(CLAUDE_IMAGE)),
        (D::Claude, K::Document) => (Some(CLAUDE_DOCUMENT), Some(PDF)),
        // Create a model response.md:201,235-253; ResponseInputImage/InputFile.
        (D::Openai, K::Image | K::Document | K::File) => (Some(ANY), Some(ANY)),
        // Create chat completion.md:180,228-236,266-280; FileRef has no URL field.
        (D::OpenaiChat, K::Image) => (Some(ANY), Some(ANY)),
        (D::OpenaiChat, K::Document | K::File) => (Some(ANY), None),
        (D::OpenaiChat, K::Audio) => (Some(CHAT_AUDIO), None),
        // Generating content.md:3513 (Blob MIME examples, not an exhaustive list),
        // 3589-3598 (FileData); Changelog.md:522 permits presigned external URLs.
        (D::Gemini, K::Video) => (
            Some(DeliveryConstraints {
                // Video understanding.md:23: inline video must be <100 MB.
                exclusive: true,
                ..GEMINI_VIDEO
            }),
            Some(GEMINI_VIDEO),
        ),
        (D::Gemini, K::Image) | (D::GeminiEmbedding, K::Image) => {
            (Some(GEMINI_IMAGE), Some(GEMINI_IMAGE))
        }
        (D::Gemini, K::Audio) => (Some(GEMINI_AUDIO), Some(GEMINI_AUDIO)),
        (D::Gemini, K::Document) => (Some(GEMINI_DOCUMENT), Some(GEMINI_DOCUMENT)),
        (D::Gemini, K::File) => (Some(GEMINI_ATTACHMENT), Some(GEMINI_ATTACHMENT)),
        _ => (None, None),
    };
    MediaCapability {
        inline,
        url,
        max_request_bytes: if target == D::Claude {
            // upstream_docs/claude/docs/API overview.md:111, direct Messages/count API.
            Some(CLAUDE_MAX_REQUEST_BYTES)
        } else {
            None
        },
    }
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
impl From<crate::Dialect> for AssetDialect {
    fn from(value: crate::Dialect) -> Self {
        match value {
            #[cfg(feature = "claude")]
            crate::Dialect::Claude => Self::Claude,
            #[cfg(feature = "openai")]
            crate::Dialect::Openai => Self::Openai,
            #[cfg(feature = "chat")]
            crate::Dialect::OpenaiChat => Self::OpenaiChat,
            #[cfg(feature = "gemini")]
            crate::Dialect::Gemini => Self::Gemini,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_size_boundaries_are_enforced_without_guessing_missing_limits() {
        let video = media_capability(AssetDialect::Gemini, MediaKind::Video);
        assert!(
            video
                .inline
                .unwrap()
                .check(Some("video/mp4"), Some(99_999_999))
                .is_ok()
        );
        assert!(
            video
                .inline
                .unwrap()
                .check(Some("video/mp4"), Some(100_000_000))
                .is_err()
        );
        assert!(
            video
                .url
                .unwrap()
                .check(Some("video/mp4"), Some(100_000_000))
                .is_ok()
        );
        assert!(
            video
                .url
                .unwrap()
                .check(Some("video/mp4"), Some(100_000_001))
                .is_err()
        );
        assert!(video.url.unwrap().check(Some("video/mp4"), None).is_ok());
        let openai = media_capability(AssetDialect::Openai, MediaKind::Image);
        assert!(
            openai
                .inline
                .unwrap()
                .check(None, Some(200_000_000))
                .is_ok()
        );
    }
}
