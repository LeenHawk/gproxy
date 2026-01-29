use time::OffsetDateTime;

use gproxy_protocol::claude::get_model::response::GetModelResponse as ClaudeGetModelResponse;
use gproxy_protocol::claude::get_model::types::ModelInfo as ClaudeModelInfo;
use gproxy_protocol::claude::list_models::types::ModelType as ClaudeModelType;
use gproxy_protocol::gemini::get_model::response::GetModelResponse as GeminiGetModelResponse;

/// Convert a Gemini get-model response into Claude's get-model response shape.
pub fn transform_response(response: GeminiGetModelResponse) -> ClaudeGetModelResponse {
    let name = response.name;
    let id = if let Some(stripped) = name.strip_prefix("models/") {
        stripped.to_string()
    } else {
        name
    };

    ClaudeModelInfo {
        id: id.clone(),
        // Gemini model metadata does not expose a created timestamp; use epoch as placeholder.
        created_at: OffsetDateTime::UNIX_EPOCH,
        display_name: response.display_name.unwrap_or(id),
        r#type: ClaudeModelType::Model,
    }
}
