use time::OffsetDateTime;

use gproxy_protocol::claude::list_models::response::ListModelsResponse as ClaudeListModelsResponse;
use gproxy_protocol::claude::list_models::types::BetaModelInfo as ClaudeModelInfo;
use gproxy_protocol::claude::list_models::types::ModelType as ClaudeModelType;
use gproxy_protocol::openai::list_models::response::ListModelsResponse as OpenAIListModelsResponse;

/// Convert an OpenAI list-models response into Claude's list-models response shape.
pub fn transform_response(response: OpenAIListModelsResponse) -> ClaudeListModelsResponse {
    let data: Vec<ClaudeModelInfo> = response
        .data
        .into_iter()
        .map(|model| ClaudeModelInfo {
            id: model.id.clone(),
            created_at: OffsetDateTime::from_unix_timestamp(model.created.unwrap_or(0))
                .unwrap_or(OffsetDateTime::UNIX_EPOCH),
            display_name: model.id,
            r#type: ClaudeModelType::Model,
        })
        .collect();

    let first_id = data.first().map(|model| model.id.clone());
    let last_id = data.last().map(|model| model.id.clone());

    ClaudeListModelsResponse {
        data,
        first_id,
        has_more: false,
        last_id,
    }
}
