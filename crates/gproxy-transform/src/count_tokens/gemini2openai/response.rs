use gproxy_protocol::gemini::count_tokens::response::CountTokensResponse as GeminiCountTokensResponse;
use gproxy_protocol::openai::count_tokens::response::InputTokenCountResponse as OpenAIInputTokenCountResponse;

/// Convert an OpenAI input-tokens response into Gemini's count-tokens response shape.
pub fn transform_response(response: OpenAIInputTokenCountResponse) -> GeminiCountTokensResponse {
    let total_tokens = clamp_i64_to_u32(response.input_tokens);

    GeminiCountTokensResponse {
        total_tokens,
        cached_content_token_count: None,
        prompt_tokens_details: None,
        cache_tokens_details: None,
    }
}

fn clamp_i64_to_u32(value: i64) -> u32 {
    if value <= 0 {
        0
    } else if value > i64::from(u32::MAX) {
        u32::MAX
    } else {
        value as u32
    }
}
