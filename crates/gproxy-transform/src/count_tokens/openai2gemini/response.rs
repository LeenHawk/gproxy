use gproxy_protocol::gemini::count_tokens::response::CountTokensResponse as GeminiCountTokensResponse;
use gproxy_protocol::openai::count_tokens::response::InputTokenCountResponse as OpenAIInputTokenCountResponse;
use gproxy_protocol::openai::count_tokens::types::InputTokenObjectType as OpenAIInputTokenObjectType;

/// Convert a Gemini count-tokens response into OpenAI's input-tokens response shape.
pub fn transform_response(response: GeminiCountTokensResponse) -> OpenAIInputTokenCountResponse {
    gproxy_protocol::openai::count_tokens::types::InputTokenCount {
        object: OpenAIInputTokenObjectType::ResponseInputTokens,
        input_tokens: i64::from(response.total_tokens),
    }
}
