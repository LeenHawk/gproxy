use gproxy_protocol::claude::get_model::response::GetModelResponse as ClaudeGetModelResponse;
use gproxy_protocol::openai::get_model::response::GetModelResponse as OpenAIGetModelResponse;
use gproxy_protocol::openai::get_model::types::{
    Model as OpenAIModel, ModelObjectType as OpenAIModelObjectType,
};

/// Convert a Claude get-model response into OpenAI's model response shape.
pub fn transform_response(response: ClaudeGetModelResponse) -> OpenAIGetModelResponse {
    OpenAIModel {
        id: response.id,
        created: Some(response.created_at.unix_timestamp()),
        object: OpenAIModelObjectType::Model,
        // Claude's model metadata does not include ownership; use a stable placeholder.
        owned_by: "unknown".to_string(),
    }
}
