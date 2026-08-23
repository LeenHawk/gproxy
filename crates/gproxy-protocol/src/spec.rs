//! The OperationSpec registry: every fact about an operation, declared once.
//!
//! v2 scattered these facts across 10+ match sites and five parallel
//! billable lists; here classification, settlement, affinity, and console
//! metadata all read one declaration. Request-body expectations are not a
//! field — they derive from the ingress method (single truth).

use http::Method;

use crate::operation::{Operation, OperationKind};

/// One path segment of an ingress pattern. Static tables, no matcher DSL:
/// the full pattern language is exactly what the known APIs need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    /// Literal segment: `v1`, `files`.
    Lit(&'static str),
    /// Capture one segment: `{id}`.
    Param(&'static str),
    /// Gemini's `models/{model}:generateContent` shape — a capture with a
    /// literal `:action` suffix in the same segment.
    ParamAction(&'static str, &'static str),
    /// Capture the whole remaining path (service-surface prefixes). Only
    /// valid as the final segment.
    Rest(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathPattern(pub &'static [Seg]);

/// How to tell a streaming request from a buffered one at this ingress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDetect {
    /// Never streams.
    Never,
    /// A boolean body field (`"stream"`) decides; classification promotes
    /// the operation to its streaming sibling when set.
    BodyFlag(&'static str),
    /// The endpoint itself is the streaming form (`:streamGenerateContent`).
    Always,
}

/// One way this operation enters the proxy.
#[derive(Debug, Clone, Copy)]
pub struct Ingress {
    pub method: &'static Method,
    pub pattern: PathPattern,
    pub kind: OperationKind,
    pub stream: StreamDetect,
    /// This ingress is a websocket upgrade (`GET /v1/realtime`,
    /// Responses-over-WS). The engine hands matched upgrades to the WS
    /// bridge instead of the HTTP path; hosts never hardcode WS routes
    /// (v2's gateway carried a three-branch if-chain for exactly this).
    pub upgrade: bool,
}

/// When the funnel settles this operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleMode {
    /// Not billable; settles at zero for telemetry only.
    Free,
    /// Settle from the response (or stream tail) usage.
    OnResponse,
    /// Async-job pattern (video): settle only when the polled body reports
    /// `status == "completed"`, deduplicated across polls.
    OnCompletedStatus,
}

/// Which credential-stickiness the operation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affinity {
    None,
    /// Conversation stickiness via session id / fingerprint.
    Session,
    /// The named resource (`"file"`, `"video"`) is bound to the credential
    /// that created it; follow-up calls must land there.
    Resource(&'static str),
}

/// Everything the engine needs to know about an operation.
#[derive(Debug, Clone, Copy)]
pub struct OperationSpec {
    pub ingress: &'static [Ingress],
    pub settle: SettleMode,
    pub affinity: Affinity,
}

/// A successful ingress match.
#[derive(Debug)]
pub struct Matched {
    pub operation: Operation,
    pub kind: OperationKind,
    pub stream: StreamDetect,
    pub upgrade: bool,
    /// Captured `Param`/`ParamAction` values, in pattern order.
    pub params: Vec<(&'static str, String)>,
}

/// Linear scan over every operation's ingress table. The table is small
/// and static; a hot-path matcher can replace the scan later without the
/// signature changing.
pub fn match_ingress(method: &Method, path: &str) -> Option<Matched> {
    let path = path.strip_prefix('/')?;

    for (operation, spec) in &crate::specs::REGISTRY {
        for ingress in spec.ingress {
            if ingress.method != method {
                continue;
            }
            if let Some(params) = match_pattern(ingress.pattern, path) {
                return Some(Matched {
                    operation: *operation,
                    kind: ingress.kind,
                    stream: ingress.stream,
                    upgrade: ingress.upgrade,
                    params,
                });
            }
        }
    }

    None
}

fn match_pattern(pattern: PathPattern, path: &str) -> Option<Vec<(&'static str, String)>> {
    let mut segments = path.split('/');
    let mut params = Vec::new();

    for pattern_segment in pattern.0 {
        let segment = segments.next()?;
        match pattern_segment {
            Seg::Lit(expected) if segment == *expected => {}
            Seg::Lit(_) => return None,
            Seg::Param(name) if !segment.is_empty() => params.push((*name, segment.to_owned())),
            Seg::Param(_) => return None,
            Seg::ParamAction(name, action) => {
                let value = segment.strip_suffix(action)?.strip_suffix(':')?;
                if value.is_empty() {
                    return None;
                }
                params.push((*name, value.to_owned()));
            }
            Seg::Rest(name) => {
                if segment.is_empty() {
                    return None;
                }
                let mut rest = segment.to_owned();
                for segment in segments {
                    if segment.is_empty() {
                        return None;
                    }
                    rest.push('/');
                    rest.push_str(segment);
                }
                params.push((*name, rest));
                return Some(params);
            }
        }
    }

    segments.next().is_none().then_some(params)
}

/// Streaming promotion: which operation a `BodyFlag` ingress becomes when
/// the flag is set. Exhaustive so a new streaming pair cannot be missed.
pub const fn streaming_sibling(operation: Operation) -> Option<Operation> {
    match operation {
        Operation::GenerateContent => Some(Operation::StreamGenerateContent),
        Operation::ListModels
        | Operation::GetModel
        | Operation::CountTokens
        | Operation::StreamGenerateContent
        | Operation::CompactContent
        | Operation::CreateEmbedding
        | Operation::Rerank
        | Operation::WebSearch
        | Operation::CreateImage
        | Operation::EditImage
        | Operation::CreateSpeech
        | Operation::CreateTranscription
        | Operation::CreateTranslation
        | Operation::CreateFile
        | Operation::ListFiles
        | Operation::RetrieveFile
        | Operation::RetrieveFileContent
        | Operation::DeleteFile
        | Operation::CreateVideo
        | Operation::RetrieveVideo
        | Operation::CreateRealtimeCall => None,
    }
}

impl Operation {
    /// The one declaration everything reads. Exhaustive: a new operation
    /// does not compile until its spec exists.
    pub fn spec(self) -> &'static OperationSpec {
        crate::specs::spec(self)
    }
}
