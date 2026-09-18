//! Native method paths the conversion targets are posted to. Passthrough uses
//! the client's own path; conversion has to know where the target dialect's
//! operation lives.

use gproxy_protocol::{Dialect, adapt::generate::Endpoint, transform::TransformError};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'/')
    .add(b'?')
    .add(b'#')
    .add(b':')
    .add(b'%');

/// The generation endpoint for `dialect`. Gemini carries the model in the
/// path and selects streaming by method; the others use a body flag.
pub fn generate_endpoint(
    dialect: Dialect,
    model: &str,
    stream: bool,
) -> Result<Endpoint, TransformError> {
    match dialect {
        Dialect::OpenAi => Endpoint::new("/v1/responses"),
        Dialect::OpenAiChat => Endpoint::new("/v1/chat/completions"),
        Dialect::Claude => Endpoint::new("/v1/messages"),
        Dialect::Gemini => {
            let model = utf8_percent_encode(model, PATH_SEGMENT);
            let method = if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            };
            let mut endpoint = Endpoint::new(format!("/v1beta/models/{model}:{method}"))?;
            if stream {
                endpoint.query = Some("alt=sse".into());
            }
            Ok(endpoint)
        }
        Dialect::OpenAiResponsesWebSocket => Err(TransformError::unsupported(
            "endpoint",
            "the Responses WebSocket dialect has no HTTP generation path",
        )),
    }
}
