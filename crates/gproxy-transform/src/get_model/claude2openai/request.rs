use gproxy_protocol::claude::get_model::request::GetModelRequest as ClaudeGetModelRequest;
use gproxy_protocol::openai::get_model::request::{
    GetModelPath as OpenAIGetModelPath, GetModelRequest as OpenAIGetModelRequest,
};

/// Convert a Claude get-model request into an OpenAI get-model request.
/// Claude-specific headers are dropped here and should be handled by the provider layer if needed.
pub fn transform_request(request: ClaudeGetModelRequest) -> OpenAIGetModelRequest {
    OpenAIGetModelRequest {
        path: OpenAIGetModelPath {
            model: request.path.model_id,
        },
    }
}
