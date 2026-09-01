use std::time::Duration;

use gproxy_protocol::{Affinity, OperationKind};
use sha2::{Digest, Sha256};

use crate::api::Core;
use crate::boundary::{RequestCtx, RoutingMode};
use crate::control::{Plan, Target};
use crate::host::{CacheBackend, Host};

use super::request::Classified;

mod fingerprint;

const TTL: Duration = Duration::from_secs(60 * 60);
const KEY_DOMAIN: &[u8] = b"gproxy:session-affinity:key:v1";
const SELECTION_DOMAIN: &[u8] = b"gproxy:session-affinity:selection:v1";
const SUBJECT_DOMAIN: &[u8] = b"gproxy:session-affinity:subject:v1";

#[derive(Clone, Copy)]
pub(super) struct SessionSubject([u8; 32]);

pub(super) struct SessionAffinity {
    key: String,
}

pub(super) fn selection_key(subject: Option<SessionSubject>, user_key_id: i64) -> i64 {
    let Some(subject) = subject else {
        return user_key_id;
    };
    let mut hasher = Sha256::new();
    field(&mut hasher, SELECTION_DOMAIN);
    field(&mut hasher, &user_key_id.to_be_bytes());
    field(&mut hasher, &subject.0);
    let digest = hasher.finalize();
    i64::from_be_bytes(digest[..8].try_into().expect("SHA-256 has eight bytes"))
}

pub(super) fn subject(
    ctx: &RequestCtx,
    kind: OperationKind,
    body: Option<&serde_json::Value>,
) -> Option<SessionSubject> {
    header(&ctx.headers, "x-gproxy-session-id")
        .map(|value| digest_subject(b"gproxy", value.as_bytes()))
        .or_else(|| match kind {
            OperationKind::ContentGeneration(
                gproxy_protocol::ContentGenerationKind::ClaudeMessages,
            ) => header(&ctx.headers, "x-claude-code-session-id")
                .or_else(|| header(&ctx.headers, "session_id"))
                .map(|value| digest_subject(b"claude-code", value.as_bytes())),
            OperationKind::ContentGeneration(
                gproxy_protocol::ContentGenerationKind::OpenAiChat
                | gproxy_protocol::ContentGenerationKind::OpenAiResponses
                | gproxy_protocol::ContentGenerationKind::OpenAiResponsesWebSocket,
            )
            | OperationKind::Family(gproxy_protocol::WireFamily::OpenAi) => {
                header(&ctx.headers, "session-id")
                    .or_else(|| header(&ctx.headers, "x-session-id"))
                    .or_else(|| header(&ctx.headers, "thread-id"))
                    .map(|value| digest_subject(b"openai", value.as_bytes()))
            }
            OperationKind::ContentGeneration(
                gproxy_protocol::ContentGenerationKind::GeminiGenerateContent,
            )
            | OperationKind::Family(
                gproxy_protocol::WireFamily::Claude | gproxy_protocol::WireFamily::Gemini,
            ) => None,
        })
        .or_else(|| body.and_then(|body| fingerprint::digest(kind, body)))
        .map(SessionSubject)
}

pub(super) async fn apply<H: Host>(
    core: &Core<H>,
    ctx: &RequestCtx,
    classified: &Classified,
    user_key_id: i64,
    plan: &mut Plan,
) -> Option<SessionAffinity> {
    if classified.key.operation.spec().affinity != Affinity::Session {
        return None;
    }
    let key = cache_key(ctx, classified.session, user_key_id);
    let pinned = match core.host.cache().get(&key).await {
        Ok(value) => value.and_then(|value| decode_target(&value)),
        Err(error) => {
            tracing::warn!(error = %error, "session affinity read failed");
            None
        }
    };
    if let Some((provider, credential)) = pinned
        && let Some(index) = plan
            .targets
            .iter()
            .position(|target| target.provider.id == provider && target.credential.0 == credential)
        && index > 0
    {
        let target = plan.targets.remove(index);
        plan.targets.insert(0, target);
    }
    Some(SessionAffinity { key })
}

impl SessionAffinity {
    pub(super) async fn commit<H: Host>(&self, core: &Core<H>, target: &Target) {
        let value = encode_target(target);
        if let Err(error) = core.host.cache().set(&self.key, value, Some(TTL)).await {
            tracing::warn!(error = %error, "session affinity commit failed");
        }
    }
}

fn cache_key(ctx: &RequestCtx, subject: Option<SessionSubject>, user_key_id: i64) -> String {
    let mut hasher = Sha256::new();
    field(&mut hasher, KEY_DOMAIN);
    field(&mut hasher, &user_key_id.to_be_bytes());
    match &ctx.mode {
        RoutingMode::Aggregated => field(&mut hasher, b"aggregated"),
        RoutingMode::Namespace { namespace } => {
            field(&mut hasher, b"namespace");
            field(&mut hasher, namespace.as_bytes());
        }
        RoutingMode::Scoped { provider } => {
            field(&mut hasher, b"provider");
            field(&mut hasher, provider.as_bytes());
        }
        RoutingMode::Named { name } => {
            field(&mut hasher, b"named");
            field(&mut hasher, name.as_bytes());
        }
    }
    match subject {
        Some(subject) => field(&mut hasher, &subject.0),
        None => field(&mut hasher, b"caller"),
    }
    format!("gproxy:session-affinity:v1:{}", hex(&hasher.finalize()))
}

fn digest_subject(kind: &[u8], value: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    field(&mut hasher, SUBJECT_DOMAIN);
    field(&mut hasher, kind);
    field(&mut hasher, value);
    hasher.finalize().into()
}

fn field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn encode_target(target: &Target) -> Vec<u8> {
    let mut value = Vec::with_capacity(16);
    value.extend_from_slice(&target.provider.id.to_be_bytes());
    value.extend_from_slice(&target.credential.0.to_be_bytes());
    value
}

fn decode_target(value: &[u8]) -> Option<(i64, i64)> {
    let provider = i64::from_be_bytes(value.get(..8)?.try_into().ok()?);
    let credential = i64::from_be_bytes(value.get(8..16)?.try_into().ok()?);
    (value.len() == 16).then_some((provider, credential))
}

fn header<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(output, "{byte:02x}").expect("write to string");
    }
    output
}
