//! Request-scoped asset preparation. The original request retains references.
//! Generic resolvers own storage/signing. The optional s3 resolver uses OpenDAL;
//! preparation never reads local files or fetches media URLs.
use crate::*;

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AssetLocation {
    Message,
    ToolResult,
    DocumentChunk,
    Embedding,
    ImageInput,
    ImageMask,
    Transcription,
}

#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResolveAsset {
    pub reference: String,
    pub target: AssetDialect,
    pub kind: MediaKind,
    pub location: AssetLocation,
    pub media_type: Option<String>,
    /// Already narrowed for this content envelope and known MIME type.
    pub capability: MediaCapability,
    pub preferred: DeliveryMethod,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ResolvedAsset {
    Bytes {
        bytes: Vec<u8>,
        media_type: String,
    },
    Url {
        url: String,
        media_type: Option<String>,
        /// Optional storage-object size, supplied by the caller. Never fetched.
        byte_len: Option<u64>,
    },
}

/// Deliberately neither Serialize nor Deref<Target=GenerateRequest>. Keep the
/// original request/Conversation as history; this snapshot contains signed URLs.
/// Sending or rendering consumes the snapshot, so each next send needs preparation.
///
/// ```compile_fail
/// use gproxy_client_sdk::PreparedGenerateRequest;
/// fn persist(prepared: PreparedGenerateRequest) {
///     let _ = serde_json::to_string(&prepared); // no Serialize implementation
/// }
/// ```
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub struct PreparedGenerateRequest {
    pub(crate) request: GenerateRequest,
    pub(crate) warnings: Vec<Warning>,
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
impl PreparedGenerateRequest {
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
    pub fn render(self, streaming: bool) -> Result<Rendered<serde_json::Value>> {
        let mut rendered = crate::dialects::render(&self.request, streaming)?;
        let mut warnings = self.warnings;
        warnings.extend(rendered.warnings);
        rendered.warnings = warnings;
        Ok(rendered)
    }
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
impl GenerateRequest {
    /// Prepare a fresh send-only snapshot. Call again for every send to obtain
    /// fresh presigned URLs. No resolved value is written into this request.
    pub async fn resolve_assets<F, Fut>(
        &self,
        mode: DeliveryMode,
        mut resolve: F,
    ) -> Result<PreparedGenerateRequest>
    where
        F: FnMut(ResolveAsset) -> Fut,
        Fut: Future<Output = Result<ResolvedAsset>>,
    {
        let mut request = self.clone();
        let target = request.model.dialect.into();
        let mut warnings = Vec::new();
        for (mi, message) in request.messages.iter_mut().enumerate() {
            for (bi, block) in message.content.iter_mut().enumerate() {
                let path = format!("messages[{mi}].content[{bi}]");
                prepare_block(
                    &mut block.content,
                    &message.role,
                    target,
                    &path,
                    mode,
                    &mut resolve,
                    &mut warnings,
                )
                .await?;
            }
        }
        Ok(PreparedGenerateRequest { request, warnings })
    }
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use super::prepare::prepare_block;
#[cfg(feature = "embeddings")]
pub(crate) use super::prepare::prepare_image;
#[cfg(any(feature = "embeddings", feature = "images", feature = "transcription"))]
pub(crate) use super::validation::check_asset;
#[cfg(feature = "claude")]
pub(crate) use super::validation::check_claude_request;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub(crate) use super::validation::validate_request;

#[cfg(any(feature = "images", feature = "transcription"))]
pub(crate) use super::prepare::prepare_media;
