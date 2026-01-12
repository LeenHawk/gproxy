use axum::http::StatusCode;

use crate::providers::endpoints::{DownstreamRequest, UpstreamRequest};
use crate::providers::router::{
    ParsedJsonResponse, ParsedSseResponse, map_parsed_json, map_parsed_sse,
};

pub(crate) fn to_upstream_request<T>(req: DownstreamRequest<T>) -> UpstreamRequest<T> {
    UpstreamRequest {
        method: req.method,
        path: req.path,
        query: req.query,
        headers: req.headers,
        body: req.body,
    }
}

pub(crate) fn map_json_response<TIn, TOut, F>(
    parsed: ParsedJsonResponse<TIn>,
    map: F,
) -> Result<ParsedJsonResponse<TOut>, StatusCode>
where
    F: FnOnce(TIn) -> Result<TOut, StatusCode>,
{
    map_parsed_json(parsed, map)
}

pub(crate) fn map_sse_response<TIn, TOut, F>(
    parsed: ParsedSseResponse<TIn>,
    map: F,
) -> ParsedSseResponse<TOut>
where
    TIn: Send + 'static,
    TOut: Send + 'static,
    F: FnMut(TIn) -> Result<TOut, StatusCode> + Send + 'static,
{
    map_parsed_sse(parsed, map)
}
