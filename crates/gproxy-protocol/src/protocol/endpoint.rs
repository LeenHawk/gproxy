//! Target endpoint synthesis (M2): the provider-relative method/path/query a
//! transformed request must hit for a given operation key. Passthrough keeps
//! the inbound target and never calls this.

use crate::protocol::operation::{
    ContentGenerationKind, HttpMethod, Operation, OperationKey, OperationKind, Provider,
};

/// Provider-relative request target for a wired operation.
#[derive(Debug, Clone, PartialEq, Eq, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RequestTarget {
    pub method: HttpMethod,
    pub path: String,
    /// Extra query the wire format requires (e.g. gemini `alt=sse`).
    pub query: Option<String>,
}

impl RequestTarget {
    fn get(path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            path: path.into(),
            query: None,
        }
    }

    fn post(path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            path: path.into(),
            query: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EndpointError {
    InconsistentOperationKey(OperationKey),
    StreamMismatch {
        operation: Operation,
        stream: bool,
    },
    UnsupportedOperation {
        operation: Operation,
        provider: Provider,
    },
    MissingModel {
        operation: Operation,
        provider: Provider,
    },
}

impl std::fmt::Display for EndpointError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InconsistentOperationKey(key) => {
                write!(formatter, "inconsistent operation key: {key:?}")
            }
            Self::StreamMismatch { operation, stream } => write!(
                formatter,
                "operation {operation:?} is incompatible with stream={stream}"
            ),
            Self::UnsupportedOperation {
                operation,
                provider,
            } => write!(
                formatter,
                "operation {operation:?} is unsupported by {provider:?}"
            ),
            Self::MissingModel {
                operation,
                provider,
            } => write!(
                formatter,
                "operation {operation:?} for {provider:?} requires a non-empty raw model id"
            ),
        }
    }
}

impl std::error::Error for EndpointError {}

/// Build the upstream request target for any wired operation key. `model` is
/// the upstream model id (path-templated providers embed it); `stream` selects
/// the streaming variant where the wire format distinguishes it by endpoint.
pub fn request_target(
    target: OperationKey,
    model: &str,
    stream: bool,
) -> Result<RequestTarget, EndpointError> {
    if !target.is_consistent() {
        return Err(EndpointError::InconsistentOperationKey(target));
    }
    use Provider as P;
    let provider = match target.kind() {
        OperationKind::ContentGeneration(kind) => {
            let operation_streams = target.operation() == Operation::StreamGenerateContent;
            if operation_streams != stream {
                return Err(EndpointError::StreamMismatch {
                    operation: target.operation(),
                    stream,
                });
            }
            return content_target(target.operation(), kind, model, stream);
        }
        OperationKind::Provider(provider) => provider,
    };
    let request_target = match (target.operation(), provider) {
        (Operation::ListModels, P::OpenAi | P::Claude) => RequestTarget::get("/v1/models"),
        (Operation::ListModels, P::Gemini) => RequestTarget::get("/v1beta/models"),
        (Operation::GetModel, P::OpenAi | P::Claude) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1/models/{}", encode_component(model)))
        }
        (Operation::GetModel, P::Gemini) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1beta/models/{}", encode_component(model)))
        }
        (Operation::CountTokens, P::OpenAi) => RequestTarget::post("/v1/responses/input_tokens"),
        (Operation::CountTokens, P::Claude) => RequestTarget::post("/v1/messages/count_tokens"),
        (Operation::CountTokens, P::Gemini) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::post(format!(
                "/v1beta/models/{}:countTokens",
                encode_component(model)
            ))
        }
        (Operation::CreateEmbedding, P::OpenAi) => RequestTarget::post("/v1/embeddings"),
        (Operation::CreateSpeech, P::OpenAi) => RequestTarget::post("/v1/audio/speech"),
        (Operation::CreateTranscription, P::OpenAi) => {
            RequestTarget::post("/v1/audio/transcriptions")
        }
        (Operation::CreateTranslation, P::OpenAi) => RequestTarget::post("/v1/audio/translations"),
        (Operation::Rerank, P::OpenAi) => RequestTarget::post("/v1/rerank"),
        // single-embed form; batch (`:batchEmbedContents`) is a separate op
        (Operation::CreateEmbedding, P::Gemini) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::post(format!(
                "/v1beta/models/{}:embedContent",
                encode_component(model)
            ))
        }
        (Operation::CreateImage, P::OpenAi) => RequestTarget::post("/v1/images/generations"),
        // Imagen-native generation; `gemini-*-image` models route through
        // generate-content instead.
        (Operation::CreateImage, P::Gemini) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::post(format!(
                "/v1beta/models/{}:predict",
                encode_component(model)
            ))
        }
        (Operation::EditImage, P::OpenAi) => RequestTarget::post("/v1/images/edits"),
        (Operation::WebSearch, P::OpenAi) => RequestTarget::post("/v1/alpha/search"),
        (Operation::CompactContent, P::OpenAi) => RequestTarget::post("/v1/responses/compact"),
        (Operation::CreateConversation, P::OpenAi) => RequestTarget::post("/v1/conversations"),
        // Video: resource identifiers (video id, operation name, file id)
        // travel in the `model` argument slot, like `GetModel`.
        (Operation::CreateVideo, P::OpenAi) => RequestTarget::post("/v1/videos"),
        (Operation::ListVideos, P::OpenAi) => RequestTarget::get("/v1/videos"),
        (Operation::RetrieveVideo, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1/videos/{}", encode_component(model)))
        }
        (Operation::DeleteVideo, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget {
                method: HttpMethod::Delete,
                path: format!("/v1/videos/{}", encode_component(model)),
                query: None,
            }
        }
        (Operation::DownloadVideoContent, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1/videos/{}/content", encode_component(model)))
        }
        (Operation::RemixVideo, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::post(format!("/v1/videos/{}/remix", encode_component(model)))
        }
        (Operation::CreateVideoCharacter, P::OpenAi) => {
            RequestTarget::post("/v1/videos/characters")
        }
        (Operation::GetVideoCharacter, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1/videos/characters/{}", encode_component(model)))
        }
        (Operation::EditVideo, P::OpenAi) => RequestTarget::post("/v1/videos/edits"),
        (Operation::ExtendVideo, P::OpenAi) => RequestTarget::post("/v1/videos/extensions"),
        (Operation::CreateVideo, P::Gemini) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::post(format!(
                "/v1beta/models/{}:predictLongRunning",
                encode_component(model)
            ))
        }
        // The poll target is the operation resource name
        // (`models/<model>/operations/<id>`); slashes are structural.
        (Operation::RetrieveVideo, P::Gemini) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1beta/{}", model.trim_start_matches('/')))
        }
        (Operation::DownloadVideoContent, P::Gemini) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget {
                method: HttpMethod::Get,
                path: format!("/v1beta/files/{}:download", encode_component(model)),
                query: Some("alt=media".to_owned()),
            }
        }
        (Operation::CreateRealtimeCall, P::OpenAi) => RequestTarget::post("/v1/realtime/calls"),
        (Operation::CreateFile, P::OpenAi) => RequestTarget::post("/v1/files"),
        (Operation::ListFiles, P::OpenAi) => RequestTarget::get("/v1/files"),
        (Operation::RetrieveFile, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1/files/{}", encode_component(model)))
        }
        (Operation::DeleteFile, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget {
                method: HttpMethod::Delete,
                path: format!("/v1/files/{}", encode_component(model)),
                query: None,
            }
        }
        (Operation::DownloadFileContent, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget::get(format!("/v1/files/{}/content", encode_component(model)))
        }
        (Operation::ConnectRealtime, P::OpenAi) => {
            require_model(target.operation(), provider, model)?;
            RequestTarget {
                method: HttpMethod::Get,
                path: "/v1/realtime".to_owned(),
                query: Some(format!("model={}", encode_component(model))),
            }
        }
        (operation, provider) => {
            return Err(EndpointError::UnsupportedOperation {
                operation,
                provider,
            });
        }
    };
    Ok(request_target)
}

/// Content-generation targets (POST; gemini selects the verb by `stream`).
fn content_target(
    operation: Operation,
    kind: ContentGenerationKind,
    model: &str,
    stream: bool,
) -> Result<RequestTarget, EndpointError> {
    use ContentGenerationKind as K;
    let target = match kind {
        K::OpenAiChatCompletions => RequestTarget::post("/v1/chat/completions"),
        K::OpenAiResponses => RequestTarget::post("/v1/responses"),
        K::OpenAiResponsesWebSocket if stream => RequestTarget::get("/v1/responses"),
        K::OpenAiResponsesWebSocket => {
            return Err(EndpointError::StreamMismatch { operation, stream });
        }
        K::ClaudeMessages => RequestTarget::post("/v1/messages"),
        K::GeminiGenerateContent => {
            require_model(operation, Provider::Gemini, model)?;
            let verb = if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            };
            RequestTarget {
                method: HttpMethod::Post,
                path: format!("/v1beta/models/{}:{verb}", encode_component(model)),
                query: stream.then(|| "alt=sse".to_owned()),
            }
        }
    };
    Ok(target)
}

fn require_model(
    operation: Operation,
    provider: Provider,
    model: &str,
) -> Result<(), EndpointError> {
    if model.is_empty() {
        Err(EndpointError::MissingModel {
            operation,
            provider,
        })
    } else {
        Ok(())
    }
}

/// Percent-encode one raw path/query component. Model ids passed to this
/// module are always raw ids, never provider paths or pre-encoded fragments.
fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_endpoint_support_matrix_is_explicit() {
        use Operation as O;
        use Provider as P;
        let rows = [
            (O::ListModels, [true, true, true]),
            (O::GetModel, [true, true, true]),
            (O::CountTokens, [true, true, true]),
            (O::CreateEmbedding, [true, false, true]),
            (O::CreateSpeech, [true, false, false]),
            (O::CreateTranscription, [true, false, false]),
            (O::CreateTranslation, [true, false, false]),
            (O::Rerank, [true, false, false]),
            (O::CreateImage, [true, false, true]),
            (O::EditImage, [true, false, false]),
            (O::WebSearch, [true, false, false]),
            (O::CompactContent, [true, false, false]),
            (O::CreateConversation, [true, false, false]),
            (O::CreateRealtimeCall, [true, false, false]),
            (O::ConnectRealtime, [true, false, false]),
            (O::CreateVideo, [true, false, true]),
            (O::RetrieveVideo, [true, false, true]),
            (O::ListVideos, [true, false, false]),
            (O::DeleteVideo, [true, false, false]),
            (O::DownloadVideoContent, [true, false, true]),
            (O::RemixVideo, [true, false, false]),
            (O::CreateVideoCharacter, [true, false, false]),
            (O::GetVideoCharacter, [true, false, false]),
            (O::EditVideo, [true, false, false]),
            (O::ExtendVideo, [true, false, false]),
        ];
        for (operation, supported) in rows {
            for (provider, expected) in [P::OpenAi, P::Claude, P::Gemini].into_iter().zip(supported)
            {
                let result =
                    request_target(OperationKey::provider(operation, provider), "model", false);
                assert_eq!(result.is_ok(), expected, "{operation:?} / {provider:?}");
            }
        }
    }

    #[test]
    fn content_endpoint_support_matrix_is_explicit() {
        use ContentGenerationKind as K;
        for kind in [
            K::OpenAiResponses,
            K::OpenAiResponsesWebSocket,
            K::OpenAiChatCompletions,
            K::ClaudeMessages,
            K::GeminiGenerateContent,
        ] {
            for (operation, stream) in [
                (Operation::GenerateContent, false),
                (Operation::StreamGenerateContent, true),
            ] {
                let result = request_target(
                    OperationKey::content_generation(operation, kind),
                    "model",
                    stream,
                );
                assert_eq!(
                    result.is_ok(),
                    kind != K::OpenAiResponsesWebSocket || stream,
                    "{operation:?} / {kind:?}"
                );
            }
        }
    }

    #[test]
    fn rejects_inconsistent_keys_and_stream_flags() {
        let inconsistent = OperationKey::new_unchecked(
            Operation::GenerateContent,
            OperationKind::Provider(Provider::OpenAi),
        );
        assert!(matches!(
            request_target(inconsistent, "model", false),
            Err(EndpointError::InconsistentOperationKey(_))
        ));

        let key = OperationKey::content_generation(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        );
        assert!(matches!(
            request_target(key, "model", false),
            Err(EndpointError::StreamMismatch { .. })
        ));
    }

    #[test]
    fn model_is_a_raw_encoded_component() {
        let key = OperationKey::provider(Operation::GetModel, Provider::Gemini);
        let target = request_target(key, "org/model ?", false).unwrap();
        assert_eq!(target.path, "/v1beta/models/org%2Fmodel%20%3F");

        let key = OperationKey::provider(Operation::ConnectRealtime, Provider::OpenAi);
        let target = request_target(key, "gpt/a b", false).unwrap();
        assert_eq!(target.query.as_deref(), Some("model=gpt%2Fa%20b"));
    }
}
