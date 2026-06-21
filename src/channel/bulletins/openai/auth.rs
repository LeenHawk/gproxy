//! OpenAI auth: `Authorization: Bearer <api_key>`.

use bytes::Bytes;
use http::Request;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

pub(super) fn apply(req: &mut Request<Bytes>, key: &str) -> Result<(), ChannelError> {
    common::inject_bearer(req, key)
}
