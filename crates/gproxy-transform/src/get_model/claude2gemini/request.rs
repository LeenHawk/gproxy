use gproxy_protocol::claude::get_model::request::GetModelRequest as ClaudeGetModelRequest;
use gproxy_protocol::gemini::get_model::request::{
    GetModelPath as GeminiGetModelPath, GetModelRequest as GeminiGetModelRequest,
};

/// Convert a Claude get-model request into a Gemini get-model request.
pub fn transform_request(request: ClaudeGetModelRequest) -> GeminiGetModelRequest {
    let name = request.path.model_id;

    GeminiGetModelRequest {
        path: GeminiGetModelPath { name },
    }
}
