use gproxy_channel_api::{TrafficBlacklistConfig, TrafficPolicyConfig};
use http::HeaderMap;

use crate::boundary::RequestCtx;

pub(crate) fn strip_ingress(ctx: &mut RequestCtx) {
    ctx.headers = gproxy_channel_api::traffic::ingress_headers(&ctx.headers);
    ctx.query = gproxy_channel_api::traffic::ingress_query(ctx.query.as_deref());
}

pub(crate) fn request_headers(
    source: &HeaderMap,
    policy: &TrafficPolicyConfig,
    blacklist: &TrafficBlacklistConfig,
) -> HeaderMap {
    policy.filter_request_headers_with(source, blacklist)
}

pub(crate) fn response_headers(
    source: HeaderMap,
    policy: &TrafficPolicyConfig,
    blacklist: &TrafficBlacklistConfig,
) -> HeaderMap {
    policy.filter_response_headers_with(source, blacklist)
}

pub(crate) fn request_query(
    query: Option<&str>,
    policy: &TrafficPolicyConfig,
    blacklist: &TrafficBlacklistConfig,
) -> Option<String> {
    policy.filter_request_query_with(query, blacklist)
}
