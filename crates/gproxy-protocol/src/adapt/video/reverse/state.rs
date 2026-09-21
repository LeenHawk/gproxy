use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReverseVideoKind {
    Native,
    OpenRouter,
}

/// Host-selected target/API facts, persisted before any publication or create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReverseVideoBinding {
    pub operation_name: String,
    pub origin: String,
    pub model: String,
    pub kind: ReverseVideoKind,
    pub api_origin_url: String,
    pub create_path: String,
    pub query_prefix: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReverseVideoRequest {
    Native(o::NativeCreateVideoRequestBody),
    OpenRouter(o::CreateVideoRequestBody),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReverseVideoResult {
    Native(o::NativeVideo),
    OpenRouter(o::VideoGenerationResponseBody),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverseVideoChild {
    pub instance_index: usize,
    pub sample_index: usize,
    pub source: g::PredictLongRunningRequestBody,
    pub request: Option<ReverseVideoRequest>,
    /// Persisted before POST. Absence of a result never permits another POST.
    pub started: bool,
    pub result: Option<ReverseVideoResult>,
    pub published: Option<g::VideoOperation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverseVideoState {
    pub schema: u32,
    pub native_defaults: Option<video::NativeVideoDefaults>,
    pub expires_at: SystemTime,
    pub binding: ReverseVideoBinding,
    pub original: g::PredictLongRunningRequestBody,
    pub children: Vec<ReverseVideoChild>,
}

#[derive(Debug, Default)]
pub struct ReverseVideoProgress {
    pub state: Option<ReverseVideoState>,
    pub version: Option<Version>,
    pub child_index: Option<usize>,
    pub send_started: bool,
    pub raw_response: Option<WireResponse<bytes::Bytes>>,
    pub result: Option<ReverseVideoResult>,
    pub publication_ids: Vec<String>,
    pub publications: Vec<ResourceReference>,
}

pub(super) fn conflict(path: &str) -> TransformError {
    TransformError::new(
        TransformErrorKind::Conflict,
        path,
        "saved creation requires reconciliation; do not repeat POST",
    )
}

impl ReverseVideoBinding {
    pub(super) fn key(&self) -> String {
        format!(
            "video/reverse/{}:{}",
            self.operation_name.len(),
            self.operation_name
        )
    }

    pub(super) fn query_path(&self, id: &str) -> Result<String, TransformError> {
        if id.is_empty() || id.chars().any(char::is_control) {
            return Err(TransformError::shape(
                "video.id",
                "nonempty control-free target identity required",
            ));
        }
        let mut encoded = String::new();
        for byte in id.bytes() {
            if byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'~')
                || byte == b'.' && !matches!(id, "." | "..")
            {
                encoded.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(&mut encoded, "%{byte:02X}").expect("String write");
            }
        }
        Ok(format!("{}{encoded}", self.query_prefix))
    }
}

impl ReverseVideoResult {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Native(v) => &v.id,
            Self::OpenRouter(v) => &v.id,
        }
    }
    pub(super) fn same_projection(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Native(a), Self::Native(b)) => {
                a.id == b.id && a.status == b.status && a.error == b.error
            }
            (Self::OpenRouter(a), Self::OpenRouter(b)) => {
                a.id == b.id
                    && a.status == b.status
                    && a.error == b.error
                    && a.unsigned_urls == b.unsigned_urls
            }
            _ => false,
        }
    }
    pub(super) fn clean(self) -> Self {
        match self {
            Self::Native(v) => Self::Native(v.into_declared()),
            Self::OpenRouter(v) => Self::OpenRouter(v.into_declared()),
        }
    }
}

pub(super) async fn save<S: StateStore>(
    store: &S,
    scope: &S::Scope,
    state: &ReverseVideoState,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<(), TransformError> {
    // Retain the known outcome even when CAS fails after a side effect.
    progress.state = Some(state.clone());
    let version = super::super::state::save_payload(
        store,
        scope,
        &state.binding.key(),
        state,
        progress.version.clone(),
        limits,
    )
    .await?;
    progress.version = Some(version);
    Ok(())
}

pub(super) async fn load<S: StateStore>(
    store: &S,
    scope: &S::Scope,
    binding: &ReverseVideoBinding,
    limits: VideoLimits,
) -> Result<(ReverseVideoState, Version), TransformError> {
    let entry = store
        .get(scope, &binding.key())
        .await?
        .ok_or_else(|| TransformError::missing_metadata("video.reverse.state"))?;
    if entry.payload.len() as u64 > store.limits().read_bytes {
        return Err(super::super::resources::limit("video.state"));
    }
    let mut state: ReverseVideoState = crate::codec::decode_json(&entry.payload, limits.codec)
        .map_err(|e| TransformError::invalid_result("video.state", e.to_string()))?;
    if state.schema != 1
        || &state.binding != binding
        || state.children.is_empty()
        || state.children.len() > limits.max_resource_facts
    {
        return Err(TransformError::shape(
            "video.state.binding",
            "invalid schema, binding or child count",
        ));
    }
    if state.expires_at <= SystemTime::now() {
        return Err(TransformError::missing_metadata(
            "video.state.unexpired_publications",
        ));
    }
    state.original = state.original.into_declared();
    for child in &mut state.children {
        child.source = std::mem::replace(
            &mut child.source,
            g::PredictLongRunningRequestBody::builder(vec![]).build(),
        )
        .into_declared();
        child.request = child.request.take().map(|v| match v {
            ReverseVideoRequest::Native(v) => ReverseVideoRequest::Native(v.into_declared()),
            ReverseVideoRequest::OpenRouter(v) => {
                ReverseVideoRequest::OpenRouter(v.into_declared())
            }
        });
        child.result = child.result.take().map(ReverseVideoResult::clean);
        child.published = child.published.take().map(DeclaredFields::into_declared);
    }
    Ok((state, entry.version))
}
