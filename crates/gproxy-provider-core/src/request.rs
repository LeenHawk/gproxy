use gproxy_protocol::claude;
use gproxy_protocol::gemini;
use gproxy_protocol::openai;
use http::HeaderMap;

#[derive(Debug, Clone)]
pub enum ProxyRequest {
    ClaudeMessages(claude::create_message::request::CreateMessageRequest),
    ClaudeMessagesStream(claude::create_message::request::CreateMessageRequest),
    ClaudeCountTokens(claude::count_tokens::request::CountTokensRequest),
    ClaudeModelsList(claude::list_models::request::ListModelsRequest),
    ClaudeModelsGet(claude::get_model::request::GetModelRequest),

    GeminiGenerate(gemini::generate_content::request::GenerateContentRequest),
    GeminiGenerateStream(gemini::stream_content::request::StreamGenerateContentRequest),
    GeminiCountTokens(gemini::count_tokens::request::CountTokensRequest),
    GeminiModelsList(gemini::list_models::request::ListModelsRequest),
    GeminiModelsGet(gemini::get_model::request::GetModelRequest),

    OpenAIChat(openai::create_chat_completions::request::CreateChatCompletionRequest),
    OpenAIChatStream(openai::create_chat_completions::request::CreateChatCompletionRequest),
    OpenAIResponses(openai::create_response::request::CreateResponseRequest),
    OpenAIResponsesStream(openai::create_response::request::CreateResponseRequest),
    OpenAIInputTokens(openai::count_tokens::request::InputTokenCountRequest),
    OpenAIModelsList(openai::list_models::request::ListModelsRequest),
    OpenAIModelsGet(openai::get_model::request::GetModelRequest),

    OAuthStart {
        query: Option<String>,
        headers: HeaderMap,
    },
    OAuthCallback {
        query: Option<String>,
        headers: HeaderMap,
    },
    Usage,
}
