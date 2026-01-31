use gproxy_provider_core::ProxyRequest;
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
    Unsupported { reason: &'static str },
}
/// Upstream usage extraction strategy for dispatch/record.
/// This represents upstream usage signals (not local stats). Providers without
/// upstream usage support should use `UsageKind::None`.
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum OperationKind {
    ClaudeMessages = 0,
    ClaudeMessagesStream = 1,
    ClaudeCountTokens = 2,
    ClaudeModelsList = 3,
    ClaudeModelsGet = 4,
    GeminiGenerate = 5,
    GeminiGenerateStream = 6,
    GeminiCountTokens = 7,
    GeminiModelsList = 8,
    GeminiModelsGet = 9,
    OpenAIChat = 10,
    OpenAIChatStream = 11,
    OpenAIResponses = 12,
    OpenAIResponsesStream = 13,
    OpenAIInputTokens = 14,
    OpenAIModelsList = 15,
    OpenAIModelsGet = 16,
    OAuthStart = 17,
    OAuthCallback = 18,
    Usage = 19,
}

impl OperationKind {
    pub const COUNT: usize = 20;

    pub fn from_request(req: &ProxyRequest) -> Self {
        match req {
            ProxyRequest::ClaudeMessages(_) => OperationKind::ClaudeMessages,
            ProxyRequest::ClaudeMessagesStream(_) => OperationKind::ClaudeMessagesStream,
            ProxyRequest::ClaudeCountTokens(_) => OperationKind::ClaudeCountTokens,
            ProxyRequest::ClaudeModelsList(_) => OperationKind::ClaudeModelsList,
            ProxyRequest::ClaudeModelsGet(_) => OperationKind::ClaudeModelsGet,
            ProxyRequest::GeminiGenerate(_) => OperationKind::GeminiGenerate,
            ProxyRequest::GeminiGenerateStream(_) => OperationKind::GeminiGenerateStream,
            ProxyRequest::GeminiCountTokens(_) => OperationKind::GeminiCountTokens,
            ProxyRequest::GeminiModelsList(_) => OperationKind::GeminiModelsList,
            ProxyRequest::GeminiModelsGet(_) => OperationKind::GeminiModelsGet,
            ProxyRequest::OpenAIChat(_) => OperationKind::OpenAIChat,
            ProxyRequest::OpenAIChatStream(_) => OperationKind::OpenAIChatStream,
            ProxyRequest::OpenAIResponses(_) => OperationKind::OpenAIResponses,
            ProxyRequest::OpenAIResponsesStream(_) => OperationKind::OpenAIResponsesStream,
            ProxyRequest::OpenAIInputTokens(_) => OperationKind::OpenAIInputTokens,
            ProxyRequest::OpenAIModelsList(_) => OperationKind::OpenAIModelsList,
            ProxyRequest::OpenAIModelsGet(_) => OperationKind::OpenAIModelsGet,
            ProxyRequest::OAuthStart { .. } => OperationKind::OAuthStart,
            ProxyRequest::OAuthCallback { .. } => OperationKind::OAuthCallback,
            ProxyRequest::Usage => OperationKind::Usage,
        }
    }

    fn as_usize(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy)]
pub enum TransformTarget {
    Gemini,
    Claude,
    OpenAIChat,
    OpenAI,
}

#[derive(Clone, Copy)]
pub enum OpMode {
    Native,
    Transform(TransformTarget),
    Unsupported,
}

#[derive(Clone, Copy)]
pub struct OpSpec {
    pub mode: OpMode,
    pub usage: UsageKind,
}

pub struct DispatchTable {
    ops: [OpSpec; OperationKind::COUNT],
}

impl DispatchTable {
    pub const fn new(ops: [OpSpec; OperationKind::COUNT]) -> Self {
        Self { ops }
    }

    pub fn spec(&self, kind: OperationKind) -> OpSpec {
        self.ops[kind.as_usize()]
    }
}

pub const fn native_spec(usage: UsageKind) -> OpSpec {
    OpSpec {
        mode: OpMode::Native,
        usage,
    }
}

pub const fn transform_spec(target: TransformTarget, usage: UsageKind) -> OpSpec {
    OpSpec {
        mode: OpMode::Transform(target),
        usage,
    }
}

pub const fn unsupported_spec() -> OpSpec {
    OpSpec {
        mode: OpMode::Unsupported,
        usage: UsageKind::None,
    }
}

pub fn dispatch_plan_from_table(req: ProxyRequest, table: &DispatchTable) -> DispatchPlan {
    let kind = OperationKind::from_request(&req);
    let spec = table.spec(kind);
    match spec.mode {
        OpMode::Native => DispatchPlan::Native {
            req,
            usage: spec.usage,
        },
        OpMode::Transform(target) => match build_transform_plan(req, target) {
            Some(plan) => DispatchPlan::Transform {
                plan,
                usage: spec.usage,
            },
            None => DispatchPlan::Unsupported {
                reason: "unsupported transform",
            },
        },
        OpMode::Unsupported => DispatchPlan::Unsupported {
            reason: "unsupported operation",
        },
    }
}

fn build_transform_plan(
    req: ProxyRequest,
    target: TransformTarget,
) -> Option<TransformPlan> {
    match req {
        ProxyRequest::ClaudeMessages(request) => match target {
            TransformTarget::Gemini => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::Claude2Gemini(request),
            )),
            TransformTarget::OpenAIChat => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::Claude2OpenAIChat(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::Claude2OpenAIResponses(request),
            )),
            _ => None,
        },
        ProxyRequest::ClaudeMessagesStream(request) => match target {
            TransformTarget::Gemini => Some(TransformPlan::StreamContent(
                StreamContentPlan::Claude2Gemini(request),
            )),
            TransformTarget::OpenAIChat => Some(TransformPlan::StreamContent(
                StreamContentPlan::Claude2OpenAIChat(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::StreamContent(
                StreamContentPlan::Claude2OpenAIResponses(request),
            )),
            _ => None,
        },
        ProxyRequest::ClaudeCountTokens(request) => match target {
            TransformTarget::Gemini => Some(TransformPlan::CountTokens(
                CountTokensPlan::Claude2Gemini(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::CountTokens(
                CountTokensPlan::Claude2OpenAIInputTokens(request),
            )),
            _ => None,
        },
        ProxyRequest::ClaudeModelsList(request) => match target {
            TransformTarget::Gemini => Some(TransformPlan::ModelsList(
                ModelsListPlan::Claude2Gemini(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::ModelsList(
                ModelsListPlan::Claude2OpenAI(request),
            )),
            _ => None,
        },
        ProxyRequest::ClaudeModelsGet(request) => match target {
            TransformTarget::Gemini => Some(TransformPlan::ModelsGet(
                ModelsGetPlan::Claude2Gemini(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::ModelsGet(
                ModelsGetPlan::Claude2OpenAI(request),
            )),
            _ => None,
        },
        ProxyRequest::GeminiGenerate(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::Gemini2Claude(request),
            )),
            TransformTarget::OpenAIChat => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::Gemini2OpenAIChat(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::Gemini2OpenAIResponses(request),
            )),
            _ => None,
        },
        ProxyRequest::GeminiGenerateStream(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::StreamContent(
                StreamContentPlan::Gemini2Claude(request),
            )),
            TransformTarget::OpenAIChat => Some(TransformPlan::StreamContent(
                StreamContentPlan::Gemini2OpenAIChat(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::StreamContent(
                StreamContentPlan::Gemini2OpenAIResponses(request),
            )),
            _ => None,
        },
        ProxyRequest::GeminiCountTokens(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::CountTokens(
                CountTokensPlan::Gemini2Claude(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::CountTokens(
                CountTokensPlan::Gemini2OpenAIInputTokens(request),
            )),
            _ => None,
        },
        ProxyRequest::GeminiModelsList(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::ModelsList(
                ModelsListPlan::Gemini2Claude(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::ModelsList(
                ModelsListPlan::Gemini2OpenAI(request),
            )),
            _ => None,
        },
        ProxyRequest::GeminiModelsGet(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::ModelsGet(
                ModelsGetPlan::Gemini2Claude(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::ModelsGet(
                ModelsGetPlan::Gemini2OpenAI(request),
            )),
            _ => None,
        },
        ProxyRequest::OpenAIChat(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::OpenAIChat2Claude(request),
            )),
            TransformTarget::Gemini => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::OpenAIChat2Gemini(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::OpenAIChat2OpenAIResponses(request),
            )),
            _ => None,
        },
        ProxyRequest::OpenAIChatStream(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::StreamContent(
                StreamContentPlan::OpenAIChat2Claude(request),
            )),
            TransformTarget::Gemini => Some(TransformPlan::StreamContent(
                StreamContentPlan::OpenAIChat2Gemini(request),
            )),
            TransformTarget::OpenAI => Some(TransformPlan::StreamContent(
                StreamContentPlan::OpenAIChat2OpenAIResponses(request),
            )),
            _ => None,
        },
        ProxyRequest::OpenAIResponses(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::OpenAIResponses2Claude(request),
            )),
            TransformTarget::Gemini => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::OpenAIResponses2Gemini(request),
            )),
            TransformTarget::OpenAIChat => Some(TransformPlan::GenerateContent(
                GenerateContentPlan::OpenAIResponses2OpenAIChat(request),
            )),
            _ => None,
        },
        ProxyRequest::OpenAIResponsesStream(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::StreamContent(
                StreamContentPlan::OpenAIResponses2Claude(request),
            )),
            TransformTarget::Gemini => Some(TransformPlan::StreamContent(
                StreamContentPlan::OpenAIResponses2Gemini(request),
            )),
            TransformTarget::OpenAIChat => Some(TransformPlan::StreamContent(
                StreamContentPlan::OpenAIResponses2OpenAIChat(request),
            )),
            _ => None,
        },
        ProxyRequest::OpenAIInputTokens(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::CountTokens(
                CountTokensPlan::OpenAIInputTokens2Claude(request),
            )),
            TransformTarget::Gemini => Some(TransformPlan::CountTokens(
                CountTokensPlan::OpenAIInputTokens2Gemini(request),
            )),
            _ => None,
        },
        ProxyRequest::OpenAIModelsList(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::ModelsList(
                ModelsListPlan::OpenAI2Claude(request),
            )),
            TransformTarget::Gemini => Some(TransformPlan::ModelsList(
                ModelsListPlan::OpenAI2Gemini(request),
            )),
            _ => None,
        },
        ProxyRequest::OpenAIModelsGet(request) => match target {
            TransformTarget::Claude => Some(TransformPlan::ModelsGet(
                ModelsGetPlan::OpenAI2Claude(request),
            )),
            TransformTarget::Gemini => Some(TransformPlan::ModelsGet(
                ModelsGetPlan::OpenAI2Gemini(request),
            )),
            _ => None,
        },
        ProxyRequest::OAuthStart { .. }
        | ProxyRequest::OAuthCallback { .. }
        | ProxyRequest::Usage => None,
    }
}

pub enum GenerateContentPlan {
    /// Claude -> Gemini (messages)
    Claude2Gemini (gproxy_protocol::claude::create_message::request::CreateMessageRequest),
    /// Claude -> OpenAI responses
    Claude2OpenAIResponses(gproxy_protocol::claude::create_message::request::CreateMessageRequest),
    /// Claude -> OpenAI chat completions
    Claude2OpenAIChat(gproxy_protocol::claude::create_message::request::CreateMessageRequest),
    /// Gemini -> Claude (generate content)
    Gemini2Claude(gemini::generate_content::request::GenerateContentRequest),
    /// Gemini -> OpenAI responses
    Gemini2OpenAIResponses(gemini::generate_content::request::GenerateContentRequest),
    /// Gemini -> OpenAI chat completions
    Gemini2OpenAIChat(gemini::generate_content::request::GenerateContentRequest),
    /// OpenAI Responses -> Claude
    OpenAIResponses2Claude(openai::create_response::request::CreateResponseRequest),
    /// OpenAI Responses -> Gemini
    OpenAIResponses2Gemini(openai::create_response::request::CreateResponseRequest),
    /// OpenAI Responses -> OpenAI chat completions
    OpenAIResponses2OpenAIChat(openai::create_response::request::CreateResponseRequest),
    /// OpenAI chat completions -> Claude
    OpenAIChat2Claude(openai::create_chat_completions::request::CreateChatCompletionRequest),
    /// OpenAI chat completions -> Gemini
    OpenAIChat2Gemini (openai::create_chat_completions::request::CreateChatCompletionRequest),
    /// OpenAI chat completions -> OpenAI Responses
    OpenAIChat2OpenAIResponses(openai::create_chat_completions::request::CreateChatCompletionRequest),
}

pub enum StreamContentPlan {
    /// Claude -> Gemini (messages stream)
    Claude2Gemini (gproxy_protocol::claude::create_message::request::CreateMessageRequest),
    /// Claude -> OpenAI responses stream
    Claude2OpenAIResponses(gproxy_protocol::claude::create_message::request::CreateMessageRequest),
    /// Claude -> OpenAI chat completions stream
    Claude2OpenAIChat(gproxy_protocol::claude::create_message::request::CreateMessageRequest),
    /// Gemini -> Claude (stream generate)
    Gemini2Claude(gemini::stream_content::request::StreamGenerateContentRequest),
    /// Gemini -> OpenAI responses stream
    Gemini2OpenAIResponses(gemini::stream_content::request::StreamGenerateContentRequest),
    /// Gemini -> OpenAI chat completions stream
    Gemini2OpenAIChat(gemini::stream_content::request::StreamGenerateContentRequest),
    /// OpenAI Responses stream -> Claude
    OpenAIResponses2Claude(openai::create_response::request::CreateResponseRequest),
    /// OpenAI Responses stream -> Gemini
    OpenAIResponses2Gemini(openai::create_response::request::CreateResponseRequest),
    /// OpenAI Responses stream -> OpenAI chat completions
    OpenAIResponses2OpenAIChat(openai::create_response::request::CreateResponseRequest),
    /// OpenAI chat completions stream -> Claude
    OpenAIChat2Claude(openai::create_chat_completions::request::CreateChatCompletionRequest),
    /// OpenAI chat completions stream -> Gemini
    OpenAIChat2Gemini (openai::create_chat_completions::request::CreateChatCompletionRequest),
    /// OpenAI chat completions stream -> OpenAI Responses
    OpenAIChat2OpenAIResponses(openai::create_chat_completions::request::CreateChatCompletionRequest),
}

pub enum CountTokensPlan {
    /// Claude -> Gemini (count tokens)
    Claude2Gemini (gproxy_protocol::claude::count_tokens::request::CountTokensRequest),
    /// Claude -> OpenAI input tokens
    Claude2OpenAIInputTokens(gproxy_protocol::claude::count_tokens::request::CountTokensRequest),
    /// Gemini -> Claude (count tokens)
    Gemini2Claude(gemini::count_tokens::request::CountTokensRequest),
    /// Gemini -> OpenAI input tokens
    Gemini2OpenAIInputTokens(gemini::count_tokens::request::CountTokensRequest),
    /// OpenAI input_tokens -> Claude
    OpenAIInputTokens2Claude(openai::count_tokens::request::InputTokenCountRequest),
    /// OpenAI input_tokens -> Gemini
    OpenAIInputTokens2Gemini(openai::count_tokens::request::InputTokenCountRequest),
}

pub enum ModelsListPlan {
    /// Claude -> Gemini (list models)
    Claude2Gemini (gproxy_protocol::claude::list_models::request::ListModelsRequest),
    /// Claude -> OpenAI (list models)
    Claude2OpenAI(gproxy_protocol::claude::list_models::request::ListModelsRequest),
    /// Gemini -> Claude (list models)
    Gemini2Claude(gemini::list_models::request::ListModelsRequest),
    /// Gemini -> OpenAI (list models)
    Gemini2OpenAI(gemini::list_models::request::ListModelsRequest),
    /// OpenAI models list -> Claude
    OpenAI2Claude(openai::list_models::request::ListModelsRequest),
    /// OpenAI models list -> Gemini
    OpenAI2Gemini(openai::list_models::request::ListModelsRequest),
}

pub enum ModelsGetPlan {
    /// Claude -> Gemini (get model)
    Claude2Gemini (gproxy_protocol::claude::get_model::request::GetModelRequest),
    /// Claude -> OpenAI (get model)
    Claude2OpenAI(gproxy_protocol::claude::get_model::request::GetModelRequest),
    /// Gemini -> Claude (get model)
    Gemini2Claude(gemini::get_model::request::GetModelRequest),
    /// Gemini -> OpenAI (get model)
    Gemini2OpenAI(gemini::get_model::request::GetModelRequest),
    /// OpenAI models get -> Claude
    OpenAI2Claude(openai::get_model::request::GetModelRequest),
    /// OpenAI models get -> Gemini
    OpenAI2Gemini(openai::get_model::request::GetModelRequest),
}

pub enum TransformPlan {
    GenerateContent(GenerateContentPlan),
    StreamContent(StreamContentPlan),
    CountTokens(CountTokensPlan),
    ModelsList(ModelsListPlan),
    ModelsGet(ModelsGetPlan),
}
