use gproxy_protocol::claude::count_tokens::response::CountTokensResponse as ClaudeCountTokensResponse;
use gproxy_protocol::gemini::count_tokens::response::CountTokensResponse as GeminiCountTokensResponse;

/// Convert a Claude count-tokens response into Gemini's count-tokens response shape.
pub fn transform_response(response: ClaudeCountTokensResponse) -> GeminiCountTokensResponse {
    GeminiCountTokensResponse {
        total_tokens: response.input_tokens,
        cached_content_token_count: None,
        prompt_tokens_details: None,
        cache_tokens_details: None,
    }
}
