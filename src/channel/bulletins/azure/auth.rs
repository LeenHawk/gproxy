//! Azure API-key auth for its OpenAI and Anthropic protocol surfaces.

use bytes::Bytes;
use http::{HeaderName, Request};

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub(super) fn apply(
    req: &mut Request<Bytes>,
    key: &str,
    anthropic: bool,
) -> Result<(), ChannelError> {
    if anthropic {
        common::inject_header(req, HeaderName::from_static("x-api-key"), key)?;
        common::inject_static(
            req,
            HeaderName::from_static("anthropic-version"),
            ANTHROPIC_VERSION,
        );
    } else {
        common::inject_header(req, HeaderName::from_static("api-key"), key)?;
    }
    Ok(())
}
