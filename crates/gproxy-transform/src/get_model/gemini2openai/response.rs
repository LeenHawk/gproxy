use gproxy_protocol::gemini::get_model::response::GetModelResponse as GeminiGetModelResponse;
use gproxy_protocol::gemini::get_model::types::Model as GeminiModel;
use gproxy_protocol::openai::get_model::response::GetModelResponse as OpenAIGetModelResponse;

/// Convert an OpenAI get-model response into Gemini's model response shape.
pub fn transform_response(response: OpenAIGetModelResponse) -> GeminiGetModelResponse {
    let id = response.id;
    let name = if id.starts_with("models/") {
        id.clone()
    } else {
        format!("models/{}", id)
    };

    GeminiModel {
        name,
        base_model_id: None,
        // OpenAI model metadata does not include a Gemini version; use a placeholder.
        version: "unknown".to_string(),
        display_name: Some(id),
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
