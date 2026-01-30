use gproxy_protocol::gemini::list_models::response::ListModelsResponse as GeminiListModelsResponse;
use gproxy_protocol::openai::get_model::types::Model as OpenAIModel;
use gproxy_protocol::openai::get_model::types::ModelObjectType as OpenAIModelObjectType;
use gproxy_protocol::openai::list_models::response::{
    ListModelsResponse as OpenAIListModelsResponse, ListObjectType as OpenAIListObjectType,
};

/// Convert a Gemini list-models response into OpenAI's list-models response shape.
pub fn transform_response(response: GeminiListModelsResponse) -> OpenAIListModelsResponse {
    let data = response
        .models
        .into_iter()
        .map(|model| {
            let name = model.name;
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
        })
        .collect();

    OpenAIListModelsResponse {
        object: OpenAIListObjectType::List,
        data,
    }
}
