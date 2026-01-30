use gproxy_protocol::gemini::get_model::response::GetModelResponse as GeminiGetModelResponse;
use gproxy_protocol::openai::get_model::response::GetModelResponse as OpenAIGetModelResponse;
use gproxy_protocol::openai::get_model::types::{
    Model as OpenAIModel, ModelObjectType as OpenAIModelObjectType,
};

/// Convert a Gemini get-model response into OpenAI's model response shape.
pub fn transform_response(response: GeminiGetModelResponse) -> OpenAIGetModelResponse {
    let name = response.name;
    let id = if let Some(stripped) = name.strip_prefix("models/") {
        stripped.to_string()
    } else {
        name
    };

    OpenAIModel {
        id,
        // Gemini model metadata does not expose a created timestamp; use 0 as a placeholder.
        created: Some(0),
        object: OpenAIModelObjectType::Model,
        owned_by: "unknown".to_string(),
    }
}
