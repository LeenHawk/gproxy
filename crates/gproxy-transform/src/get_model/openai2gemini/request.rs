use gproxy_protocol::gemini::get_model::request::{
    GetModelPath as GeminiGetModelPath, GetModelRequest as GeminiGetModelRequest,
};
use gproxy_protocol::openai::get_model::request::GetModelRequest as OpenAIGetModelRequest;

/// Convert an OpenAI get-model request into a Gemini get-model request.
pub fn transform_request(request: OpenAIGetModelRequest) -> GeminiGetModelRequest {
    let name = request.path.model;

    GeminiGetModelRequest {
        path: GeminiGetModelPath { name },
    }
}
