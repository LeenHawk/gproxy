//! Codex HTTP request adaptations. The Core already selected the model.
pub(super) mod images;
mod responses;
pub(super) mod tools;

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;

pub(super) fn request(body: &Bytes) -> Result<(Bytes, tools::Aliases), ChannelError> {
    responses::request(body)
}

fn invalid(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::InvalidConfig(format!("Codex Responses request: {error}"))
}
