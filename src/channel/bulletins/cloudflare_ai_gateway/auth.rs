//! Cloudflare REST API auth: `Authorization: Bearer <Cloudflare API token>`.

use bytes::Bytes;
use http::Request;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

pub(super) fn apply(req: &mut Request<Bytes>, token: &str) -> Result<(), ChannelError> {
    common::inject_bearer(req, token)
}
