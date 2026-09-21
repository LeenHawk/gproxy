//! The three surfaces on one origin, the key each wants, and the session the
//! tool ties a conversation to.

use super::config::{OpenCodeConfig, Tier};
use crate::channel::{
    ChannelError, ChannelHeaders, CredentialView, HeaderAllowlist, PrepareContext, forwardable,
};
use crate::channels::shared::cache;
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

/// The header the `opencode` CLI keys a conversation on (v3 `policy.rs`
/// `OPENCODE`, which was the one client header v3 forwarded). It is declared
/// so a provider allow-list cannot silence a client's own conversation id.
pub const SESSION_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["x-opencode-session"],
    prefixes: &[],
};

const SESSION_HEADER: &str = "x-opencode-session";

/// The key this credential presents: a pasted `api_key`, or the account
/// token a device login produced (v3 `opencode/auth.rs::api_key`).
pub(super) fn api_key<'a>(credential: &CredentialView<'a>) -> Result<&'a str, ChannelError> {
    ["api_key", "access_token"]
        .into_iter()
        .find_map(|name| credential.secret.get(name).and_then(Value::as_str))
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}

/// A public fact the login recorded: host metadata first, then the secret's
/// `provider_fields`, then the flat layout v3 wrote.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    fn text(value: Option<&Value>) -> Option<&str> {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
    text(credential.metadata.get(name))
        .or_else(|| {
            text(
                credential
                    .secret
                    .pointer(&format!("/provider_fields/{name}")),
            )
        })
        .or_else(|| text(credential.secret.get(name)))
}

/// Each dialect's own conversation endpoint, plus the catalogue. The client's
/// path is not trusted: the Zen and Go origins mount everything at a fixed
/// place (v3 `opencode/model.rs::path`).
fn path(operation: OperationKey) -> Result<&'static str, ChannelError> {
    match (operation.operation, operation.dialect) {
        (Operation::ListModels, _) => Ok("/models"),
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAiChat) => {
            Ok("/chat/completions")
        }
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket,
        ) => Ok("/responses"),
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::Claude) => {
            Ok("/messages")
        }
        _ => Err(ChannelError::UnsupportedOperation(operation)),
    }
}

/// A fresh 32-character hex conversation id.
///
/// v3 fell back to core's session id when the client sent none; v4's
/// `PrepareContext` does not carry one, so a client that names no
/// conversation gets a new one per request, exactly as v3 did when core had
/// none either. A client that wants affinity sends the header itself.
fn minted_session() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ChannelError::InvalidConfig(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    use std::fmt::Write as _;
    Ok(bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(&mut out, "{byte:02x}");
        out
    }))
}

/// Set the conversation header when the forwarded request has none.
fn apply_session(headers: &mut HeaderMap) -> Result<(), ChannelError> {
    let named = headers
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    if named {
        return Ok(());
    }
    headers.insert(
        HeaderName::from_static(SESSION_HEADER),
        HeaderValue::from_str(&minted_session()?).map_err(|_| ChannelError::InvalidCredential)?,
    );
    Ok(())
}

pub(super) fn build(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
    let config = OpenCodeConfig::from_view(ctx.provider)?;
    let key = api_key(&ctx.credential)?;
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => format!("{}{}", config.base_url(ctx.provider), path(ctx.operation)?),
    };
    let uri = match ctx
        .request
        .query
        .as_deref()
        .map(strip_query_auth)
        .filter(|query| !query.is_empty())
    {
        Some(query) => format!("{url}?{query}"),
        None => url,
    };
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, SESSION_HEADERS)?;
    let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
    if matches!(
        ctx.operation.operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) {
        apply_session(&mut headers)?;
    }
    if ctx.operation.dialect == Dialect::Claude {
        // The Claude surface takes the same key the way Anthropic does.
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
        );
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
    } else {
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
    }
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    let body = match ctx.request.body {
        HttpBody::Bytes(bytes) => {
            if !bytes.is_empty() {
                headers
                    .entry(header::CONTENT_TYPE)
                    .or_insert_with(|| HeaderValue::from_static("application/json"));
            }
            HttpBody::Bytes(shape(bytes, &config, ctx.operation.dialect))
        }
        other => other,
    };
    let mut builder = http::Request::builder()
        .method(ctx.request.method.clone())
        .uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    builder
        .body(body)
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
}

/// Magic cache breakpoints, per the provider's opt-in.
///
/// v3 offered only the OpenAI switch here because its pipeline placed Claude
/// breakpoints centrally, after the per-provider pass. v4 has no central
/// pass, so both families are the channel's to place and both switches are
/// offered (v3 `opencode/shape.rs`).
fn shape(bytes: Bytes, config: &OpenCodeConfig, dialect: Dialect) -> Bytes {
    cache::shape(
        bytes,
        cache::rules_for(
            dialect,
            config.enable_claude_magic_cache,
            config.enable_openai_magic_cache,
        ),
    )
}

/// The tier a provider row fronts, for the quota surface that only one of
/// them has.
pub(super) fn tier(config: &OpenCodeConfig, provider: crate::channel::ProviderView<'_>) -> Tier {
    if config.tier == Tier::Go {
        return Tier::Go;
    }
    // An operator who pointed `base_url` at the Go origin stated the tier
    // just as plainly as the key would have (v3 `opencode::is_go`).
    match provider.base_url.map(str::trim) {
        Some(base) if base.trim_end_matches('/').ends_with("/zen/go/v1") => Tier::Go,
        _ => Tier::Zen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_dialect_keeps_its_own_conversation_endpoint() {
        let key = |operation, dialect| OperationKey { operation, dialect };
        assert_eq!(
            path(key(Operation::GenerateContent, Dialect::OpenAiChat)).unwrap(),
            "/chat/completions"
        );
        assert_eq!(
            path(key(Operation::StreamGenerateContent, Dialect::OpenAi)).unwrap(),
            "/responses"
        );
        assert_eq!(
            path(key(Operation::GenerateContent, Dialect::Claude)).unwrap(),
            "/messages"
        );
        assert_eq!(
            path(key(Operation::ListModels, Dialect::OpenAiChat)).unwrap(),
            "/models"
        );
        assert!(path(key(Operation::CreateImage, Dialect::OpenAi)).is_err());
    }

    #[test]
    fn a_minted_conversation_id_is_thirty_two_hex_characters() {
        let session = minted_session().unwrap();
        assert_eq!(session.len(), 32);
        assert!(session.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(session, minted_session().unwrap());
    }
}
