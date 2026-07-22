use bytes::Bytes;
use http::Request;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

pub(super) fn apply(req: &mut Request<Bytes>, api_key: &str) -> Result<(), ChannelError> {
    common::inject_bearer(req, api_key)
}
