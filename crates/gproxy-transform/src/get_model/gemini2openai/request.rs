use gproxy_protocol::gemini::get_model::request::GetModelRequest as GeminiGetModelRequest;
use gproxy_protocol::openai::get_model::request::{
    GetModelPath as OpenAIGetModelPath, GetModelRequest as OpenAIGetModelRequest,
};

/// Convert a Gemini get-model request into an OpenAI get-model request.
pub fn transform_request(request: GeminiGetModelRequest) -> OpenAIGetModelRequest {
    let model = request
        .path
        .name
        .strip_prefix("models/")
        .unwrap_or(&request.path.name)
        .to_string();

    OpenAIGetModelRequest {
        path: OpenAIGetModelPath { model },
    }
}
