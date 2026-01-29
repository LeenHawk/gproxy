use gproxy_protocol::claude::count_tokens::response::{
    BetaCountTokensContextManagementResponse, CountTokensResponse as ClaudeCountTokensResponse,
};
use gproxy_protocol::gemini::count_tokens::response::CountTokensResponse as GeminiCountTokensResponse;

/// Convert a Gemini count-tokens response into Claude's count-tokens response shape.
pub fn transform_response(response: GeminiCountTokensResponse) -> ClaudeCountTokensResponse {
    let input_tokens = response.total_tokens;

    ClaudeCountTokensResponse {
        context_management: Some(BetaCountTokensContextManagementResponse {
            original_input_tokens: input_tokens,
        }),
        input_tokens,
    }
}
