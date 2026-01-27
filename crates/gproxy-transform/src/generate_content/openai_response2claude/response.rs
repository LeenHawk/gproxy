use gproxy_protocol::claude::create_message::response::CreateMessageResponse as ClaudeCreateMessageResponse;
use gproxy_protocol::openai::create_response::response::Response as OpenAIResponse;

/// Convert a Claude create-message response into an OpenAI responses response.
pub fn transform_response(response: ClaudeCreateMessageResponse) -> OpenAIResponse {
    crate::generate_content::claude2openai_response::response::transform_response(response)
}
