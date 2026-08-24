use std::collections::VecDeque;

use crate::api::Core;
use crate::boundary::ByteStream;
use crate::error::CoreError;
use crate::funnel::FunnelCtx;
use crate::host::Host;

pub(super) struct FinalStream {
    pub pending: VecDeque<bytes::Bytes>,
    pub codec: Box<dyn gproxy_channel_api::OperationStream>,
    pub cleanup: gproxy_channel_api::PreparedRequest,
    pub ttl_secs: u64,
    pub url: String,
}

pub(super) fn wrap_response<H: Host>(
    core: &Core<H>,
    channel: &'static str,
    facts: &FunnelCtx,
    response: http::Response<ByteStream>,
    final_stream: FinalStream,
) -> Result<http::Response<ByteStream>, CoreError> {
    let FinalStream {
        pending,
        codec,
        cleanup,
        ttl_secs,
        url,
    } = final_stream;
    let (parts, body) = response.into_parts();
    if !parts.status.is_success() {
        super::cleanup::spawn_request(
            core.host.clone(),
            facts.target.clone(),
            format!("{}:cleanup", facts.request_id),
            cleanup,
        );
        return Ok(http::Response::from_parts(parts, body));
    }
    let scope = stream_scope(
        channel,
        facts,
        parts.status,
        parts.headers.clone(),
        url,
        cleanup,
        ttl_secs,
    )?;
    Ok(http::Response::from_parts(
        parts,
        super::stream::wrap(core.host.clone(), body, pending, codec, scope),
    ))
}

pub(super) fn stream_scope(
    channel: &'static str,
    facts: &FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    upstream_url: String,
    cleanup: gproxy_channel_api::PreparedRequest,
    ttl_secs: u64,
) -> Result<super::stream::Scope, CoreError> {
    Ok(super::stream::Scope {
        channel,
        owner_user_id: facts.owner_user_id.ok_or(CoreError::Unsupported)?,
        target: facts.target.clone(),
        generation: facts.request_id.clone(),
        status,
        headers,
        upstream_url,
        cleanup,
        ttl_secs,
    })
}
