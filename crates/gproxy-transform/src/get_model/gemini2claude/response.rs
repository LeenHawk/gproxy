use gproxy_protocol::claude::get_model::response::GetModelResponse as ClaudeGetModelResponse;
use gproxy_protocol::gemini::get_model::response::GetModelResponse as GeminiGetModelResponse;
use gproxy_protocol::gemini::get_model::types::Model as GeminiModel;

/// Convert a Claude get-model response into Gemini's model response shape.
pub fn transform_response(response: ClaudeGetModelResponse) -> GeminiGetModelResponse {
    let name = if response.id.starts_with("models/") {
        response.id.clone()
    } else {
        format!("models/{}", response.id)
    };

    GeminiModel {
        name,
        base_model_id: None,
        // Claude model metadata does not include a Gemini version; use a placeholder.
        version: "unknown".to_string(),
        display_name: Some(response.display_name),
        description: None,
        input_token_limit: None,
        output_token_limit: None,
        supported_generation_methods: None,
        thinking: None,
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
    }
}
