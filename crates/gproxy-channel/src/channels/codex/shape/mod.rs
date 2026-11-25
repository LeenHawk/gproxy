//! Codex HTTP request adaptations. The Core already selected the model.
pub(super) mod images;
mod responses;
pub(super) mod tools;

use gproxy_protocol::connection::Bytes;
use gproxy_upstream::UpstreamError;

pub(super) fn request(
    body: &Bytes,
    headers: &http::HeaderMap,
) -> Result<(Bytes, tools::Aliases), UpstreamError> {
    responses::request(body, headers)
}

fn invalid(error: impl std::fmt::Display) -> UpstreamError {
    UpstreamError::InvalidRequest(format!("Codex Responses request: {error}"))
}
