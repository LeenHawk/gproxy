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

/// The count-tokens endpoint for `dialect`. OpenAI is deliberately not a
/// count-tokens target: the Responses input-token endpoint is not converted to.
#[cfg(not(target_arch = "wasm32"))]
pub fn count_tokens_endpoint(dialect: Dialect, model: &str) -> Result<Endpoint, TransformError> {
    match dialect {
        Dialect::Claude => Endpoint::new("/v1/messages/count_tokens"),
        Dialect::Gemini => {
            let model = utf8_percent_encode(model, PATH_SEGMENT);
            Endpoint::new(format!("/v1beta/models/{model}:countTokens"))
        }
        other => Err(TransformError::unsupported(
            "endpoint",
            format!("{other:?} is not a count-tokens conversion target"),
        )),
    }
}

/// The model directory path for `dialect`.
#[cfg(not(target_arch = "wasm32"))]
pub fn list_models_path(dialect: Dialect) -> Result<&'static str, TransformError> {
    match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::Claude => Ok("/v1/models"),
        Dialect::Gemini => Ok("/v1beta/models"),
        Dialect::OpenAiResponsesWebSocket => Err(TransformError::unsupported(
            "endpoint",
            "the Responses WebSocket dialect has no model directory path",
        )),
    }
}

/// The single-model path for `dialect`; `id` is the bare model id without a
/// Gemini `models/` resource prefix.
#[cfg(not(target_arch = "wasm32"))]
pub fn get_model_path(dialect: Dialect, id: &str) -> Result<String, TransformError> {
    let base = list_models_path(dialect)?;
    Ok(format!("{base}/{}", utf8_percent_encode(id, PATH_SEGMENT)))
}

/// The embeddings endpoint for `dialect` as a body-less request template.
/// Gemini carries the model in the path and distinguishes single from batch
/// by method name; OpenAI has one path whose body carries the model.
#[cfg(not(target_arch = "wasm32"))]
pub fn embedding_template(
    dialect: Dialect,
    model: &str,
    batch: bool,
) -> Result<gproxy_protocol::WireRequest<()>, TransformError> {
    let path = match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat => "/v1/embeddings".to_owned(),
        Dialect::Gemini => {
            let model = utf8_percent_encode(model, PATH_SEGMENT);
            let method = if batch {
                "batchEmbedContents"
            } else {
                "embedContent"
            };
            format!("/v1beta/models/{model}:{method}")
        }
        Dialect::Claude | Dialect::OpenAiResponsesWebSocket => {
            return Err(TransformError::unsupported(
                "endpoint",
                format!("{dialect:?} has no embeddings path"),
            ));
        }
    };
    Ok(gproxy_protocol::WireRequest {
        method: http::Method::POST,
        path,
        query: None,
        headers: http::HeaderMap::new(),
        body: (),
    })
}
