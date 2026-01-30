use gproxy_protocol::claude::list_models::response::ListModelsResponse as ClaudeListModelsResponse;
use gproxy_protocol::openai::get_model::types::Model as OpenAIModel;
use gproxy_protocol::openai::get_model::types::ModelObjectType as OpenAIModelObjectType;
use gproxy_protocol::openai::list_models::response::{
    ListModelsResponse as OpenAIListModelsResponse, ListObjectType as OpenAIListObjectType,
};

/// Convert a Claude list-models response into OpenAI's list-models response shape.
pub fn transform_response(response: ClaudeListModelsResponse) -> OpenAIListModelsResponse {
    let data = response
        .data
        .into_iter()
        .map(|model| OpenAIModel {
            id: model.id,
            created: Some(model.created_at.unix_timestamp()),
            object: OpenAIModelObjectType::Model,
            // Claude model metadata does not include ownership; use a stable placeholder.
            owned_by: "unknown".to_string(),
        })
        .collect();

    OpenAIListModelsResponse {
        object: OpenAIListObjectType::List,
        data,
    }
}
