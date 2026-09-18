//! Request-side preparation shared by every attempt: bounded buffering of a
//! streaming body so retries can replay it, per-attempt cloning, and the
//! provider/credential views the channel binding needs.

use crate::{CredentialData, CredentialVersion, ProviderData};
use futures_util::StreamExt;
use gproxy_channel::channel::{CredentialView, ProviderView};
use gproxy_protocol::{HttpBody, WireRequest, connection::Bytes};

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
        version: version.version,
        expires_at_ms: version.expires_at_ms,
    }
}
