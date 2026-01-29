use time::OffsetDateTime;

use gproxy_protocol::claude::list_models::response::ListModelsResponse as ClaudeListModelsResponse;
use gproxy_protocol::claude::list_models::types::BetaModelInfo as ClaudeModelInfo;
use gproxy_protocol::claude::list_models::types::ModelType as ClaudeModelType;
use gproxy_protocol::gemini::list_models::response::ListModelsResponse as GeminiListModelsResponse;

/// Convert a Gemini list-models response into Claude's list-models response shape.
pub fn transform_response(response: GeminiListModelsResponse) -> ClaudeListModelsResponse {
    let data: Vec<ClaudeModelInfo> = response
        .models
        .into_iter()
        .map(|model| {
            let name = model.name;
            let id = if let Some(stripped) = name.strip_prefix("models/") {
                stripped.to_string()
            } else {
                name
            };

            ClaudeModelInfo {
                id: id.clone(),
                // Gemini model metadata does not expose a created timestamp; use epoch as placeholder.
                created_at: OffsetDateTime::UNIX_EPOCH,
                display_name: model.display_name.unwrap_or(id),
                r#type: ClaudeModelType::Model,
            }
        })
        .collect();

    let first_id = data.first().map(|model| model.id.clone());
    let last_id = data.last().map(|model| model.id.clone());

    ClaudeListModelsResponse {
        data,
        first_id,
        has_more: response.next_page_token.is_some(),
        last_id,
    }
}
