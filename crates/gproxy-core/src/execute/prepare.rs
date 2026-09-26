//! Request-side preparation shared by every attempt: bounded buffering of a
//! streaming body so retries can replay it, per-attempt cloning, and the
//! provider/credential views the channel binding needs.

use crate::{CredentialData, CredentialVersion, ProviderData};
use futures_util::StreamExt;
use gproxy_channel::channel::{CredentialView, ProviderView};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes,
    transform::generate::claude_gemini::without_thinking_handles,
};

/// A streaming body is buffered up to `max_bytes` so it can be replayed. Past
/// the cap the consumed prefix is chained back in front of the rest and the
/// request becomes single-attempt. Returns whether the body is replayable.
pub(crate) async fn buffer_request(
    request: WireRequest<HttpBody>,
    want_replay: bool,
    max_bytes: u64,
) -> (WireRequest<HttpBody>, bool) {
    let WireRequest {
        method,
        path,
        query,
        headers,
        body,
    } = request;
    let (body, replayable) = match body {
        HttpBody::Bytes(bytes) => (HttpBody::Bytes(bytes), true),
        HttpBody::Stream(stream) if !want_replay => (HttpBody::Stream(stream), false),
        HttpBody::Stream(mut stream) => {
            let mut collected: Vec<Bytes> = Vec::new();
            let mut total = 0u64;
            let mut overflow = None;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(chunk) => {
                        total += chunk.len() as u64;
                        collected.push(chunk);
                        if total > max_bytes {
                            overflow = Some(stream);
                            break;
                        }
                    }
                    Err(error) => {
                        // Replay the prefix and surface the error where the
                        // upstream would have seen it.
                        let prefix = futures_util::stream::iter(collected.into_iter().map(Ok));
                        let failed = futures_util::stream::iter([Err(error)]);
                        return (
                            WireRequest {
                                method,
                                path,
                                query,
                                headers,
                                body: HttpBody::Stream(Box::pin(prefix.chain(failed))),
                            },
                            false,
                        );
                    }
                }
            }
            match overflow {
                Some(rest) => {
                    let prefix = futures_util::stream::iter(collected.into_iter().map(Ok));
                    (HttpBody::Stream(Box::pin(prefix.chain(rest))), false)
                }
                None => {
                    let mut joined = Vec::with_capacity(total as usize);
                    for chunk in collected {
                        joined.extend_from_slice(&chunk);
                    }
                    (HttpBody::Bytes(Bytes::from(joined)), true)
                }
            }
        }
    };
    (
        WireRequest {
            method,
            path,
            query,
            headers,
            body,
        },
        replayable,
    )
}

/// Only buffered bodies can be sent again.
pub(crate) fn clone_request(request: &WireRequest<HttpBody>) -> Option<WireRequest<HttpBody>> {
    match &request.body {
        HttpBody::Bytes(bytes) => Some(WireRequest {
            method: request.method.clone(),
            path: request.path.clone(),
            query: request.query.clone(),
            headers: request.headers.clone(),
            body: HttpBody::Bytes(bytes.clone()),
        }),
        HttpBody::Stream(_) => None,
    }
}

/// A Claude request sent to its upstream as it is must not carry the
/// thinking blocks gproxy signed with its own handle (a Gemini upstream's
/// thinking shown to this client earlier): Anthropic cannot verify them, so
/// they are dropped. A converted request is handled by the conversion.
pub(crate) fn drop_thinking_handles(operation: OperationKey, request: &mut WireRequest<HttpBody>) {
    if operation.dialect != Dialect::Claude
        || !matches!(
            operation.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent | Operation::CountTokens
        )
    {
        return;
    }
    let HttpBody::Bytes(bytes) = &request.body else {
        return;
    };
    if let Some(body) = without_thinking_handles(bytes) {
        request.headers.remove(http::header::CONTENT_LENGTH);
        request.body = HttpBody::Bytes(Bytes::from(body));
    }
}

pub(crate) fn provider_view(provider: &ProviderData) -> ProviderView<'_> {
    crate::assemble::provider_view(&provider.entity)
}

pub(crate) fn credential_view<'a>(
    credential: &'a CredentialData,
    version: &'a CredentialVersion,
) -> CredentialView<'a> {
    CredentialView {
        id: &credential.id,
        provider_id: &credential.provider_id,
        auth_kind: &credential.auth_kind,
        secret: &version.secret,
        metadata: &credential.metadata,
        version: version.version,
        expires_at_ms: version.expires_at_ms,
    }
}

/// Bind the resolved model on a native JSON request without changing any other
/// wire bytes. Gemini also carries its model in the request path.
pub(crate) fn apply_model(
    request: &mut WireRequest<HttpBody>,
    dialect: gproxy_protocol::Dialect,
    model: &str,
) {
    if let HttpBody::Bytes(bytes) = &request.body
        && let Ok(text) = std::str::from_utf8(bytes)
        && let Some(rewritten) = crate::rewrite::json_path::rewrite_at_paths(
            text,
            &[vec![crate::PathSegment::Key("model".into())]],
            &mut |old| (old != model).then(|| model.to_owned()),
        )
    {
        request.body = HttpBody::Bytes(Bytes::from(rewritten));
        request.headers.remove(http::header::CONTENT_LENGTH);
    }
    if dialect == gproxy_protocol::Dialect::Gemini
        && let Some((prefix, tail)) = request.path.split_once("/models/")
    {
        let suffix = tail.find(':').map(|at| &tail[at..]).unwrap_or("");
        let model = model.strip_prefix("models/").unwrap_or(model);
        const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
            .remove(b'-')
            .remove(b'_')
            .remove(b'.')
            .remove(b'~');
        let encoded = percent_encoding::utf8_percent_encode(model, SEGMENT);
        request.path = format!("{prefix}/models/{encoded}{suffix}");
    }
}
