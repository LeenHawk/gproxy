use time::OffsetDateTime;

use gproxy_protocol::claude::get_model::response::GetModelResponse as ClaudeGetModelResponse;
use gproxy_protocol::claude::get_model::types::ModelInfo as ClaudeModelInfo;
use gproxy_protocol::claude::list_models::types::ModelType as ClaudeModelType;
use gproxy_protocol::openai::get_model::response::GetModelResponse as OpenAIGetModelResponse;

/// Convert an OpenAI get-model response into Claude's get-model response shape.
pub fn transform_response(response: OpenAIGetModelResponse) -> ClaudeGetModelResponse {
    let created_at = OffsetDateTime::from_unix_timestamp(response.created)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);

    ClaudeGetModelResponse {
        request_id: None,
        model: ClaudeModelInfo {
            id: response.id.clone(),
            created_at,
            display_name: response.id,
            r#type: ClaudeModelType::Model,
        },
    }
}
