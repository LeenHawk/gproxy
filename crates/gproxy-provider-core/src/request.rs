use gproxy_protocol::claude;
use gproxy_protocol::gemini;
use gproxy_protocol::openai;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeminiApiVersion {
    V1,
    V1Beta,
}

#[derive(Debug, Clone)]
pub enum ProxyRequest {
    ClaudeMessages(claude::create_message::request::CreateMessageRequest),
    ClaudeMessagesStream(claude::create_message::request::CreateMessageRequest),
    ClaudeCountTokens(claude::count_tokens::request::CountTokensRequest),
    ClaudeModelsList(claude::list_models::request::ListModelsRequest),
    ClaudeModelsGet(claude::get_model::request::GetModelRequest),

    GeminiGenerate {
        version: GeminiApiVersion,
        request: gemini::generate_content::request::GenerateContentRequest,
    },
    GeminiGenerateStream {
        version: GeminiApiVersion,
        request: gemini::stream_content::request::StreamGenerateContentRequest,
    },
    GeminiCountTokens {
        version: GeminiApiVersion,
        request: gemini::count_tokens::request::CountTokensRequest,
    },
    GeminiModelsList {
        version: GeminiApiVersion,
        request: gemini::list_models::request::ListModelsRequest,
    },
    GeminiModelsGet {
        version: GeminiApiVersion,
        request: gemini::get_model::request::GetModelRequest,
    },

    OpenAIChat(openai::create_chat_completions::request::CreateChatCompletionRequest),
    OpenAIChatStream(openai::create_chat_completions::request::CreateChatCompletionRequest),
    OpenAIResponses(openai::create_response::request::CreateResponseRequest),
    OpenAIResponsesStream(openai::create_response::request::CreateResponseRequest),
    OpenAIInputTokens(openai::count_tokens::request::InputTokenCountRequest),
    OpenAIModelsList(openai::list_models::request::ListModelsRequest),
    OpenAIModelsGet(openai::get_model::request::GetModelRequest),
}
