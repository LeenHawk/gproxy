use gproxy_protocol::claude::count_tokens::response::{
    BetaCountTokensContextManagementResponse, CountTokensResponse as ClaudeCountTokensResponse,
};
use gproxy_protocol::openai::count_tokens::response::InputTokenCountResponse as OpenAIInputTokenCountResponse;

/// Convert an OpenAI input-tokens response into Claude's count-tokens response shape.
pub fn transform_response(response: OpenAIInputTokenCountResponse) -> ClaudeCountTokensResponse {
    let input_tokens = clamp_i64_to_u32(response.input_tokens);

    ClaudeCountTokensResponse {
        context_management: Some(BetaCountTokensContextManagementResponse {
            original_input_tokens: input_tokens,
        }),
        input_tokens,
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
