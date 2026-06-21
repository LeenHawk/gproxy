//! Claude auth: `x-api-key: <api_key>` + a fixed `anthropic-version`.

use bytes::Bytes;
use http::HeaderName;
use http::Request;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub(super) fn apply(req: &mut Request<Bytes>, key: &str) -> Result<(), ChannelError> {
    common::inject_header(req, HeaderName::from_static("x-api-key"), key)?;
    common::inject_static(
        req,
        HeaderName::from_static("anthropic-version"),
        ANTHROPIC_VERSION,
    );
    Ok(())
}
