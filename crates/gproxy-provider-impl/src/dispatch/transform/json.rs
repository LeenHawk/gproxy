use bytes::Bytes;
use http::header::{CONTENT_LENGTH, TRANSFER_ENCODING};
use http::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Serialize;

use gproxy_provider_core::{
    build_downstream_event, DownstreamContext, ProxyResponse, UpstreamPassthroughError,
};

fn scrub_headers(headers: &mut HeaderMap) {
    headers.remove(CONTENT_LENGTH);
    headers.remove(TRANSFER_ENCODING);
}

#[allow(clippy::result_large_err)]
pub(super) fn transform_json_response<T, U>(
    response: ProxyResponse,
    ctx: DownstreamContext,
    transform: fn(T) -> U,
) -> Result<ProxyResponse, UpstreamPassthroughError>
where
    T: DeserializeOwned,
    U: Serialize,
{
    match response {
        ProxyResponse::Json { status, mut headers, body } => {
            let parsed = serde_json::from_slice::<T>(&body)
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            let mapped = transform(parsed);
            let mapped_body = serde_json::to_vec(&mapped)
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            scrub_headers(&mut headers);
            if let Some(meta) = ctx.downstream_meta {
                let event = build_downstream_event(
                    Some(ctx.trace_id.clone()),
                    meta,
                    status,
                    &headers,
                    Some(&Bytes::from(mapped_body.clone())),
                    false,
                );
                ctx.traffic.record_downstream(event);
            }
            Ok(ProxyResponse::Json {
                status,
                headers,
                body: Bytes::from(mapped_body),
            })
        }
        ProxyResponse::Stream { .. } => Err(UpstreamPassthroughError::service_unavailable(
            "expected json response".to_string(),
        )),
    }
}
