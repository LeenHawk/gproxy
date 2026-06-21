//! Vercel auth: sends the key as BOTH `Authorization: Bearer` and `x-api-key`
//! (the gateway accepts either depending on the upstream protocol).

use bytes::Bytes;
use http::HeaderName;
use http::Request;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

pub(super) fn apply(req: &mut Request<Bytes>, key: &str) -> Result<(), ChannelError> {
    common::inject_bearer(req, key)?;
    common::inject_header(req, HeaderName::from_static("x-api-key"), key)
}
