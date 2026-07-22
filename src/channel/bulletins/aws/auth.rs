use bytes::Bytes;
use http::{HeaderName, Request};

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

pub(super) fn apply(
    req: &mut Request<Bytes>,
    api_key: &str,
    anthropic_mantle: bool,
) -> Result<(), ChannelError> {
    if anthropic_mantle {
        common::inject_header(req, HeaderName::from_static("x-api-key"), api_key)?;
        common::inject_static(
            req,
            HeaderName::from_static("anthropic-version"),
            "2023-06-01",
        );
    } else {
        common::inject_bearer(req, api_key)?;
    }
    Ok(())
}
