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

/// The files collection for `dialect`: list and multipart upload post here,
/// retrieve/delete append the file id.
#[cfg(not(target_arch = "wasm32"))]
pub fn files_endpoint(dialect: Dialect) -> Result<Endpoint, TransformError> {
    match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::Claude => Endpoint::new("/v1/files"),
        Dialect::Gemini => Endpoint::new("/v1beta/files"),
        Dialect::OpenAiResponsesWebSocket => Err(TransformError::unsupported(
            "endpoint",
            "the Responses WebSocket dialect has no files path",
        )),
    }
}

/// Gemini's resumable upload start path; the session URL comes from upstream.
#[cfg(not(target_arch = "wasm32"))]
pub fn gemini_upload_endpoint() -> Result<Endpoint, TransformError> {
    Endpoint::new("/upload/v1beta/files")
}

/// One file's own path (retrieve/delete): the collection plus the bare id.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_endpoint(dialect: Dialect, id: &str) -> Result<Endpoint, TransformError> {
    let collection = files_endpoint(dialect)?;
    let id = utf8_percent_encode(id, PATH_SEGMENT);
    Endpoint::new(format!("{}/{id}", collection.path))
}

/// One file's content download. Gemini files are input-only upstream.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_content_endpoint(dialect: Dialect, id: &str) -> Result<Endpoint, TransformError> {
    match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::Claude => {
            let id = utf8_percent_encode(id, PATH_SEGMENT);
            Endpoint::new(format!("/v1/files/{id}/content"))
        }
        Dialect::Gemini => Err(TransformError::unsupported(
            "endpoint",
            "Gemini files cannot be downloaded",
        )),
        Dialect::OpenAiResponsesWebSocket => Err(TransformError::unsupported(
            "endpoint",
            "the Responses WebSocket dialect has no files path",
        )),
    }
}
