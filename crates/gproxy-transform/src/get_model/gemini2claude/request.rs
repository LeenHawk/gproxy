use gproxy_protocol::claude::get_model::request::{
    GetModelHeaders as ClaudeGetModelHeaders, GetModelPath as ClaudeGetModelPath,
    GetModelRequest as ClaudeGetModelRequest,
};
use gproxy_protocol::gemini::get_model::request::GetModelRequest as GeminiGetModelRequest;

/// Convert a Gemini get-model request into a Claude get-model request.
pub fn transform_request(request: GeminiGetModelRequest) -> ClaudeGetModelRequest {
    let model_id = request
        .path
        .name
        .strip_prefix("models/")
        .unwrap_or(&request.path.name)
        .to_string();

    ClaudeGetModelRequest {
        path: ClaudeGetModelPath { model_id },
        headers: ClaudeGetModelHeaders::default(),
    }
}
