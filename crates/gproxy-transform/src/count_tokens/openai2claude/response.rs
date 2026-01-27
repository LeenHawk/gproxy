use gproxy_protocol::claude::count_tokens::response::CountTokensResponse as ClaudeCountTokensResponse;
use gproxy_protocol::openai::count_tokens::response::InputTokenCountResponse as OpenAIInputTokenCountResponse;
use gproxy_protocol::openai::count_tokens::types::InputTokenObjectType as OpenAIInputTokenObjectType;

/// Convert a Claude count-tokens response into OpenAI's input-tokens response shape.
pub fn transform_response(response: ClaudeCountTokensResponse) -> OpenAIInputTokenCountResponse {
    let input_tokens = i64::from(response.input_tokens);

    gproxy_protocol::openai::count_tokens::types::InputTokenCount {
        object: OpenAIInputTokenObjectType::ResponseInputTokens,
        input_tokens,
    }
}
