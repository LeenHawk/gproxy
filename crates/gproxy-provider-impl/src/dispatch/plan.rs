use gproxy_provider_core::{GeminiApiVersion, ProxyRequest};
use gproxy_protocol::{gemini, openai};

#[derive(Clone, Copy)]
pub enum UsageKind {
    None,
    ClaudeMessage,
    GeminiGenerate,
    OpenAIChat,
    OpenAIResponses,
}

pub enum DispatchPlan {
    Native { req: ProxyRequest, usage: UsageKind },
    Transform { plan: TransformPlan, usage: UsageKind },
}

pub enum GenerateContentPlan {
    /// Claude -> Gemini (messages)
    Claude2Gemini {
        version: GeminiApiVersion,
        request: gproxy_protocol::claude::create_message::request::CreateMessageRequest,
    },
    /// Gemini -> Claude (generate content)
    Gemini2Claude(gemini::generate_content::request::GenerateContentRequest),
    /// OpenAI Responses -> Claude
    OpenAIResponses2Claude(openai::create_response::request::CreateResponseRequest),
    /// OpenAI Responses -> Gemini
    OpenAIResponses2Gemini {
        version: GeminiApiVersion,
        request: openai::create_response::request::CreateResponseRequest,
    },
}

pub enum StreamContentPlan {
    /// Claude -> Gemini (messages stream)
    Claude2Gemini {
        version: GeminiApiVersion,
        request: gproxy_protocol::claude::create_message::request::CreateMessageRequest,
    },
    /// Gemini -> Claude (stream generate)
    Gemini2Claude(gemini::stream_content::request::StreamGenerateContentRequest),
    /// OpenAI Responses stream -> Claude
    OpenAIResponses2Claude(openai::create_response::request::CreateResponseRequest),
    /// OpenAI Responses stream -> Gemini
    OpenAIResponses2Gemini {
        version: GeminiApiVersion,
        request: openai::create_response::request::CreateResponseRequest,
    },
}

pub enum CountTokensPlan {
    /// Claude -> Gemini (count tokens)
    Claude2Gemini {
        version: GeminiApiVersion,
        request: gproxy_protocol::claude::count_tokens::request::CountTokensRequest,
    },
    /// Gemini -> Claude (count tokens)
    Gemini2Claude(gemini::count_tokens::request::CountTokensRequest),
    /// OpenAI input_tokens -> Claude
    OpenAIInputTokens2Claude(openai::count_tokens::request::InputTokenCountRequest),
    /// OpenAI input_tokens -> Gemini
    OpenAIInputTokens2Gemini {
        version: GeminiApiVersion,
        request: openai::count_tokens::request::InputTokenCountRequest,
    },
}

pub enum ModelsListPlan {
    /// Claude -> Gemini (list models)
    Claude2Gemini {
        version: GeminiApiVersion,
        request: gproxy_protocol::claude::list_models::request::ListModelsRequest,
    },
    /// Gemini -> Claude (list models)
    Gemini2Claude(gemini::list_models::request::ListModelsRequest),
    /// OpenAI models list -> Claude
    OpenAI2Claude(openai::list_models::request::ListModelsRequest),
    /// OpenAI models list -> Gemini
    OpenAI2Gemini {
        version: GeminiApiVersion,
        request: openai::list_models::request::ListModelsRequest,
    },
}

pub enum ModelsGetPlan {
    /// Claude -> Gemini (get model)
    Claude2Gemini {
        version: GeminiApiVersion,
        request: gproxy_protocol::claude::get_model::request::GetModelRequest,
    },
    /// Gemini -> Claude (get model)
    Gemini2Claude(gemini::get_model::request::GetModelRequest),
    /// OpenAI models get -> Claude
    OpenAI2Claude(openai::get_model::request::GetModelRequest),
    /// OpenAI models get -> Gemini
    OpenAI2Gemini {
        version: GeminiApiVersion,
        request: openai::get_model::request::GetModelRequest,
    },
}

pub enum TransformPlan {
    GenerateContent(GenerateContentPlan),
    StreamContent(StreamContentPlan),
    CountTokens(CountTokensPlan),
    ModelsList(ModelsListPlan),
    ModelsGet(ModelsGetPlan),
}

pub(super) fn upstream_usage_for_plan(plan: &TransformPlan) -> UsageKind {
    match plan {
        TransformPlan::GenerateContent(plan) => match plan {
            GenerateContentPlan::Claude2Gemini { .. } => UsageKind::GeminiGenerate,
            GenerateContentPlan::Gemini2Claude(_) => UsageKind::ClaudeMessage,
            GenerateContentPlan::OpenAIResponses2Claude(_) => UsageKind::ClaudeMessage,
            GenerateContentPlan::OpenAIResponses2Gemini { .. } => UsageKind::GeminiGenerate,
        },
        TransformPlan::StreamContent(plan) => match plan {
            StreamContentPlan::Claude2Gemini { .. } => UsageKind::GeminiGenerate,
            StreamContentPlan::Gemini2Claude(_) => UsageKind::ClaudeMessage,
            StreamContentPlan::OpenAIResponses2Claude(_) => UsageKind::ClaudeMessage,
            StreamContentPlan::OpenAIResponses2Gemini { .. } => UsageKind::GeminiGenerate,
        },
        TransformPlan::CountTokens(_) => UsageKind::None,
        TransformPlan::ModelsList(_) => UsageKind::None,
        TransformPlan::ModelsGet(_) => UsageKind::None,
    }
}
