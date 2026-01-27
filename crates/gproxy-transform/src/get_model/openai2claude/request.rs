use gproxy_protocol::claude::get_model::request::{
    GetModelHeaders as ClaudeGetModelHeaders, GetModelPath as ClaudeGetModelPath,
    GetModelRequest as ClaudeGetModelRequest,
};
use gproxy_protocol::openai::get_model::request::GetModelRequest as OpenAIGetModelRequest;

/// Convert an OpenAI get-model request into a Claude get-model request.
/// OpenAI does not define Claude-specific headers, so we initialize defaults here.
pub fn transform_request(request: OpenAIGetModelRequest) -> ClaudeGetModelRequest {
    ClaudeGetModelRequest {
        path: ClaudeGetModelPath {
            // No mapping table yet; passthrough the identifier.
            model_id: request.path.model,
        },
        headers: ClaudeGetModelHeaders::default(),
    }
}
