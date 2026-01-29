use gproxy_protocol::claude::list_models::response::ListModelsResponse as ClaudeListModelsResponse;
use gproxy_protocol::gemini::list_models::response::ListModelsResponse as GeminiListModelsResponse;
use gproxy_protocol::gemini::types::Model as GeminiModel;

/// Convert a Claude list-models response into Gemini's list-models response shape.
pub fn transform_response(response: ClaudeListModelsResponse) -> GeminiListModelsResponse {
    let models: Vec<GeminiModel> = response
        .data
        .into_iter()
        .map(|model| {
            let id = model.id;
            let name = if id.starts_with("models/") {
                id.clone()
            } else {
                format!("models/{}", id)
            };

            GeminiModel {
                name,
                base_model_id: None,
                // Claude model metadata does not include a Gemini version; use a placeholder.
                version: "unknown".to_string(),
                display_name: Some(model.display_name),
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
        })
        .collect();

    GeminiListModelsResponse {
        models,
        next_page_token: None,
    }
}
