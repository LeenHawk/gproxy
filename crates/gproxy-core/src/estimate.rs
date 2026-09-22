//! Local token estimation for exchanges whose upstream reported no usage, or
//! only part of it. Input tokens come from the text the request carried,
//! counted with the tokenizer selected for the upstream model (a custom
//! vocabulary from the model catalog, the Setting default, or the bundled
//! encoders); output tokens are a coarse half-of-bytes guess over the
//! response body. Everything estimated is marked `estimated=true` and
//! `Partial`; reported values are never overwritten.
//!
//! The request body captured on an exchange is the native upstream request,
//! so its shape is known from the exchange's [`OperationKey`]. Text is pulled
//! through the protocol wire types for that pair — prompts, messages, tool
//! calls and results, tool declarations — and only from the positions that
//! carry human-facing text: base64 media, file ids, URLs, signatures and
//! encrypted blobs are never counted. There is deliberately no key-name
//! fallback: a pair without an extractor, or a body the wire types reject,
//! yields no input estimate rather than a number built from guessed keys.

use gproxy_channel::channel::{NormalizedUsage, UsageCompleteness};
use gproxy_protocol::{
    Dialect, Operation, OperationKey,
    wire::{claude, gemini, openai},
};
use gproxy_tokenizer::{Tokenizer, Vocabulary};
use std::collections::HashMap;

/// Parsed custom vocabularies by file id plus which model uses which.
#[derive(Default)]
pub struct Estimator {
    vocabularies: HashMap<String, Vocabulary>,
    default_file: Option<String>,
    /// `(provider_id, upstream_name)` -> vocabulary file id.
    models: HashMap<(String, String), String>,
}

impl Estimator {
    pub(crate) fn new(
        vocabularies: HashMap<String, Vocabulary>,
        default_file: Option<String>,
        models: HashMap<(String, String), String>,
    ) -> Self {
        Self {
            vocabularies,
            default_file,
            models,
        }
    }

    /// Vocabularies already parsed, for reuse across reloads.
    pub(crate) fn vocabulary(&self, file_id: &str) -> Option<&Vocabulary> {
        self.vocabularies.get(file_id)
    }

    fn tokenizer(&self, provider_id: &str, model: Option<&str>) -> Tokenizer {
        let Some(model) = model else {
            return Tokenizer::character_estimate();
        };
        let file = self
            .models
            .get(&(provider_id.to_owned(), model.to_owned()))
            .or(self.default_file.as_ref())
            .and_then(|id| self.vocabularies.get(id));
        Tokenizer::for_model(model, file).unwrap_or_else(|_| Tokenizer::character_estimate())
    }

    /// Fill the token counts the upstream did not report. `operation` is the
    /// native key of the exchange, which names the wire shape of
    /// `request_body`; `response_bytes` is what was read of the body, whole
    /// or streamed.
    pub(crate) fn complete(
        &self,
        operation: OperationKey,
        provider_id: &str,
        model: Option<&str>,
        request_body: Option<&[u8]>,
        response_bytes: u64,
        reported: Option<NormalizedUsage>,
    ) -> Option<NormalizedUsage> {
        let mut usage = reported.unwrap_or_default();
        let mut estimated = false;
        if usage.tokens.input_tokens.is_none()
            && let Some(body) = request_body
            && let Some(text) = request_text(operation, body)
        {
            let tokenizer = self.tokenizer(provider_id, model);
            if let Ok(count) = tokenizer.count(&text) {
                usage.tokens.input_tokens = Some(count);
                estimated = true;
            }
        }
        // Coarse by design: the response is JSON or SSE framing around the
        // generated text, and half its bytes is a rough stand-in for the
        // tokens inside. Refining it needs per-dialect stream parsing, which
        // the usage observer already does whenever the upstream reports.
        if usage.tokens.output_tokens.is_none() && response_bytes > 0 {
            usage.tokens.output_tokens = Some(response_bytes.div_ceil(2));
            estimated = true;
        }
        if estimated {
            usage.completeness = UsageCompleteness::Partial;
            usage
                .dimensions
                .insert("estimated".to_owned(), "true".to_owned());
            Some(usage)
        } else if usage.tokens.input_tokens.is_some() || usage.tokens.output_tokens.is_some() {
            Some(usage)
        } else {
            None
        }
    }
}

/// The human-facing text of a request, in document order, joined by
/// newlines, read through the wire types of the exchange's
/// `(operation, dialect)` pair. `None` when the pair has no extractor, the
/// body does not deserialise as that pair's wire type, or the body carries
/// no text.
fn request_text(operation: OperationKey, body: &[u8]) -> Option<String> {
    let mut text = Text::default();
    wire_text(operation, body, &mut text)?;
    (!text.0.is_empty()).then(|| text.0.join("\n"))
}

/// Collected strings, one per prompt-bearing field.
#[derive(Default)]
struct Text(Vec<String>);

impl Text {
    fn push(&mut self, text: &str) {
        if !text.is_empty() {
            self.0.push(text.to_owned());
        }
    }

    fn push_opt(&mut self, text: Option<&String>) {
        if let Some(text) = text {
            self.push(text);
        }
    }

    /// Structured tool arguments, schemas and opaque data items count as the
    /// JSON the model sees; empty containers and `null` add nothing.
    fn push_json<T: serde::Serialize>(&mut self, value: &T) {
        if let Ok(json) = serde_json::to_string(value)
            && !matches!(json.as_str(), "null" | "{}" | "[]" | "\"\"")
        {
            self.0.push(json);
        }
    }
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Option<T> {
    serde_json::from_slice(body).ok()
}

/// Extract through the wire types of the pair. `None` means there is no
/// extractor for the pair or the body did not deserialise. The websocket
/// envelope flattens a Responses body under `type: response.create`, so it
/// shares the Responses extractors through `pair_key`.
fn wire_text(operation: OperationKey, body: &[u8], out: &mut Text) -> Option<()> {
    use Operation::*;
    match (operation.pair_key().dialect, operation.operation) {
        (Dialect::Claude, GenerateContent | StreamGenerateContent) => {
            let b: claude::generate_content::GenerateContentRequestBody = parse(body)?;
            claude_request(b.system.as_ref(), &b.messages, b.tools.as_deref(), out);
        }
        (Dialect::Claude, CountTokens) => {
            let b: claude::count_tokens::CountTokensRequestBody = parse(body)?;
            claude_request(b.system.as_ref(), &b.messages, b.tools.as_deref(), out);
        }
        (Dialect::OpenAi, GenerateContent | StreamGenerateContent) => {
            let b: openai::responses::GenerateContentRequestBody = parse(body)?;
            responses_request(
                b.instructions.as_ref().and_then(Option::as_deref),
                b.input.as_ref(),
                b.tools.as_deref(),
                out,
            );
            for value in b
                .prompt
                .iter()
                .flatten()
                .flat_map(|prompt| prompt.variables.iter().flatten().flatten())
                .map(|(_, value)| value)
            {
                match value {
                    openai::responses::PromptVariableValue::String(text) => out.push(text),
                    openai::responses::PromptVariableValue::Text(text) => out.push(&text.text),
                    // Image and file variables reference media, not text.
                    _ => {}
                }
            }
        }
        (Dialect::OpenAi, CountTokens) => {
            let b: openai::count_tokens::CountTokensRequestBody = parse(body)?;
            responses_request(
                b.instructions.as_ref().and_then(Option::as_deref),
                b.input.as_ref().and_then(Option::as_ref),
                b.tools.as_ref().and_then(Option::as_deref),
                out,
            );
        }
        (Dialect::OpenAi, GuardianReview | GuardianClassify) => {
            let b: openai::guardian::GuardianRequestBody = parse(body)?;
            out.push(&b.instructions);
            b.input.iter().for_each(|item| client_item(item, out));
            if let Some(Some(tools)) = &b.tools {
                out.push_json(tools);
            }
        }
        (Dialect::OpenAi, CompactContent) => {
            // Native compaction carries the client's Responses-item history;
            // the server-shaped body is what a Responses upstream accepts.
            if let Some(b) = parse::<openai::compact::ClientCompactRequestBody>(body) {
                out.push(&b.instructions);
                b.input.iter().for_each(|item| client_item(item, out));
                if let Some(tools) = &b.tools {
                    out.push_json(tools);
                }
            } else {
                let b: openai::compact::CompactRequestBody = parse(body)?;
                responses_request(
                    b.instructions.as_ref().and_then(Option::as_deref),
                    b.input.as_ref().and_then(Option::as_ref),
                    None,
                    out,
                );
            }
        }
        (Dialect::OpenAi, SummarizeMemory) => {
            let b: openai::memory::MemorySummarizeRequestBody = parse(body)?;
            // Trace items are arbitrary JSON by contract and the summary
            // prompt embeds them as data, so their JSON is what is counted.
            for trace in &b.traces {
                out.push(&trace.metadata.source_path);
                trace.items.iter().for_each(|item| out.push_json(item));
            }
        }
        (Dialect::OpenAi, CreateEmbedding) => {
            use openai::embeddings::EmbeddingInput;
            let b: openai::embeddings::CreateEmbeddingRequestBody = parse(body)?;
            match &b.input {
                EmbeddingInput::Text(text) => out.push(text),
                EmbeddingInput::Texts(texts) => texts.iter().for_each(|t| out.push(t)),
                // Pre-tokenised input has no text to count.
                EmbeddingInput::Tokens(_) | EmbeddingInput::TokenArrays(_) => {}
            }
        }
        (Dialect::OpenAiChat, GenerateContent | StreamGenerateContent) => {
            let b: openai::chat::GenerateContentRequestBody = parse(body)?;
            chat_request(&b, out);
        }
        (Dialect::Gemini, GenerateContent | StreamGenerateContent) => {
            let b: gemini::GenerateContentRequestBody = parse(body)?;
            gemini_request(
                b.system_instruction.as_ref(),
                &b.contents,
                b.tools.as_deref(),
                out,
            );
        }
        (Dialect::Gemini, CountTokens) => {
            let b: gemini::CountTokensRequestBody = parse(body)?;
            gemini_request(None, b.contents.as_deref().unwrap_or_default(), None, out);
            if let Some(embedded) = &b.generate_content_request {
                gemini_request(
                    embedded.system_instruction.as_ref(),
                    &embedded.contents,
                    embedded.tools.as_deref(),
                    out,
                );
            }
        }
        (Dialect::Gemini, CreateEmbedding) => {
            let b: gemini::embeddings::EmbedContentRequestBody = parse(body)?;
            out.push_opt(b.title.as_ref());
            gemini_content(&b.content, out);
        }
        (Dialect::Gemini, BatchCreateEmbedding) => {
            let b: gemini::embeddings::BatchEmbedContentsRequestBody = parse(body)?;
            for request in &b.requests {
                out.push_opt(request.title.as_ref());
                gemini_content(&request.content, out);
            }
        }
        // Models, files, images, audio, video, rerank, search, conversations
        // and realtime carry no prompt text the tokenizer should meter, and
        // the dialects not listed above never serve the listed operations.
        _ => return None,
    }
    Some(())
}

// ---- Claude Messages -------------------------------------------------------

fn claude_request(
    system: Option<&claude::count_tokens::SystemPrompt>,
    messages: &[claude::content::Message],
    tools: Option<&[claude::tools::ToolUnion]>,
    out: &mut Text,
) {
    use claude::content::MessageContent;
    use claude::count_tokens::SystemPrompt;
    use claude::tools::ToolUnion;
    match system {
        Some(SystemPrompt::Text(text)) => out.push(text),
        Some(SystemPrompt::Blocks(blocks)) => blocks.iter().for_each(|b| out.push(&b.text)),
        None => {}
    }
    for message in messages {
        match &message.content {
            MessageContent::Text(text) => out.push(text),
            MessageContent::Blocks(blocks) => blocks.iter().for_each(|b| claude_block(b, out)),
        }
    }
    for tool in tools.unwrap_or_default() {
        // Built-in tools (bash, computer, web search, ...) are named by type
        // and carry no schema the model reads as text.
        if let ToolUnion::Custom(tool) = tool {
            out.push(&tool.name);
            out.push_opt(tool.description.as_ref());
            out.push_json(&tool.input_schema);
        }
    }
}

fn claude_block(block: &claude::content::ContentBlock, out: &mut Text) {
    use claude::content::{ContentBlock, McpResultContent, ToolResultContent};
    match block {
        ContentBlock::Text(b) => out.push(&b.text),
        ContentBlock::Thinking(b) => out.push(&b.thinking),
        ContentBlock::ToolUse(b) => {
            out.push(&b.name);
            out.push_json(&b.input);
        }
        ContentBlock::ServerToolUse(b) => out.push_json(&b.input),
        ContentBlock::McpToolUse(b) => {
            out.push(&b.name);
            out.push_json(&b.input);
        }
        ContentBlock::ToolResult(b) => match &b.content {
            Some(ToolResultContent::Text(text)) => out.push(text),
            Some(ToolResultContent::Blocks(blocks)) => {
                blocks.iter().for_each(|b| claude_result_block(b, out));
            }
            None => {}
        },
        ContentBlock::McpToolResult(b) => match &b.content {
            Some(McpResultContent::Text(text)) => out.push(text),
            Some(McpResultContent::Blocks(blocks)) => {
                blocks.iter().for_each(|b| out.push(&b.text));
            }
            None => {}
        },
        ContentBlock::Document(b) => claude_document(b, out),
        ContentBlock::SearchResult(b) => claude_search_result(b, out),
        // Images, redacted thinking, container uploads, compaction markers,
        // server tool results and tool add/remove notices carry base64,
        // encrypted state or ids: nothing the tokenizer should see.
        _ => {}
    }
}

fn claude_result_block(block: &claude::content::ToolResultContentBlock, out: &mut Text) {
    use claude::content::ToolResultContentBlock;
    match block {
        ToolResultContentBlock::Text(b) => out.push(&b.text),
        ToolResultContentBlock::Document(b) => claude_document(b, out),
        ToolResultContentBlock::SearchResult(b) => claude_search_result(b, out),
        ToolResultContentBlock::Image(_)
        | ToolResultContentBlock::BrowserState(_)
        | ToolResultContentBlock::ToolReference(_) => {}
    }
}

fn claude_document(block: &claude::content::DocumentBlock, out: &mut Text) {
    use claude::content::{DocumentContent, DocumentContentBlock, DocumentSource};
    out.push_opt(block.title.as_ref());
    out.push_opt(block.context.as_ref());
    match &block.source {
        DocumentSource::Text(source) => out.push(&source.data),
        DocumentSource::Content(source) => match &source.content {
            DocumentContent::Text(text) => out.push(text),
            DocumentContent::Blocks(blocks) => {
                for block in blocks {
                    if let DocumentContentBlock::Text(b) = block {
                        out.push(&b.text);
                    }
                }
            }
        },
        // Base64 PDFs, URLs and file ids are not prompt text.
        _ => {}
    }
}

fn claude_search_result(block: &claude::content::SearchResultBlock, out: &mut Text) {
    out.push(&block.title);
    block.content.iter().for_each(|b| out.push(&b.text));
}

// ---- OpenAI Chat Completions ----------------------------------------------

fn chat_request(body: &openai::chat::GenerateContentRequestBody, out: &mut Text) {
    use openai::chat::{
        AssistantContentPart, AssistantContentParts, ChatMessage, ChatTool, MessageToolCall,
        UserContentPart, UserContentParts,
    };
    for message in &body.messages {
        match message {
            ChatMessage::Developer(m) => chat_text_content(&m.content, out),
            ChatMessage::System(m) => chat_text_content(&m.content, out),
            ChatMessage::User(m) => match &m.content {
                UserContentParts::Text(text) => out.push(text),
                UserContentParts::Parts(parts) => {
                    for part in parts {
                        // image_url, input_audio and file parts are media.
                        if let UserContentPart::Text(part) = part {
                            out.push(&part.text);
                        }
                    }
                }
            },
            ChatMessage::Assistant(m) => {
                match m.content.as_ref().and_then(Option::as_ref) {
                    Some(AssistantContentParts::Text(text)) => out.push(text),
                    Some(AssistantContentParts::Parts(parts)) => {
                        for part in parts {
                            match part {
                                AssistantContentPart::Text(part) => out.push(&part.text),
                                AssistantContentPart::Refusal(part) => out.push(&part.refusal),
                            }
                        }
                    }
                    None => {}
                }
                out.push_opt(m.refusal.as_ref().and_then(Option::as_ref));
                for call in m.tool_calls.iter().flatten() {
                    match call {
                        MessageToolCall::Function(call) => {
                            out.push(&call.function.name);
                            out.push(&call.function.arguments);
                        }
                        MessageToolCall::Custom(call) => {
                            out.push(&call.custom.name);
                            out.push(&call.custom.input);
                        }
                    }
                }
                if let Some(Some(call)) = &m.function_call {
                    out.push(&call.name);
                    out.push(&call.arguments);
                }
            }
            ChatMessage::Tool(m) => chat_text_content(&m.content, out),
            ChatMessage::Function(m) => out.push_opt(m.content.as_ref()),
        }
    }
    for tool in body.tools.iter().flatten() {
        match tool {
            ChatTool::Function(tool) => {
                out.push(&tool.function.name);
                out.push_opt(tool.function.description.as_ref());
                if let Some(parameters) = &tool.function.parameters {
                    out.push_json(parameters);
                }
            }
            ChatTool::Custom(tool) => {
                out.push(&tool.custom.name);
                out.push_opt(tool.custom.description.as_ref());
            }
        }
    }
    for function in body.functions.iter().flatten() {
        out.push(&function.name);
        out.push_opt(function.description.as_ref());
        if let Some(parameters) = &function.parameters {
            out.push_json(parameters);
        }
    }
}

fn chat_text_content(content: &openai::chat::StringOrTextParts, out: &mut Text) {
    use openai::chat::StringOrTextParts;
    match content {
        StringOrTextParts::Text(text) => out.push(text),
        StringOrTextParts::Parts(parts) => parts.iter().for_each(|p| out.push(&p.text)),
    }
}

// ---- OpenAI Responses (also count-tokens and server-shaped compaction) ----

fn responses_request(
    instructions: Option<&str>,
    input: Option<&openai::responses::Input>,
    tools: Option<&[openai::responses::Tool]>,
    out: &mut Text,
) {
    use openai::responses::{Input, Tool};
    if let Some(instructions) = instructions {
        out.push(instructions);
    }
    match input {
        Some(Input::Text(text)) => out.push(text),
        Some(Input::Items(items)) => items.iter().for_each(|i| responses_item(i, out)),
        None => {}
    }
    for tool in tools.unwrap_or_default() {
        // Hosted tools (web search, file search, code interpreter, ...) are
        // configured by ids and options, not prompt text.
        if let Tool::Function(tool) = tool {
            out.push(&tool.name);
            out.push_opt(tool.description.as_ref().and_then(Option::as_ref));
            if let Some(parameters) = &tool.parameters {
                out.push_json(parameters);
            }
        }
    }
}

fn responses_item(item: &openai::responses::InputItem, out: &mut Text) {
    use openai::responses::{
        CustomOutput, FunctionOutput, FunctionOutputContent, InputItem, MessageContent,
        OutputContent,
    };
    match item {
        InputItem::OutputMessage(m) => {
            for content in &m.content {
                match content {
                    OutputContent::Text(text) => out.push(&text.text),
                    OutputContent::Refusal(refusal) => out.push(&refusal.refusal),
                }
            }
        }
        InputItem::Message(m) => responses_input_contents(&m.content, out),
        InputItem::Easy(m) => match &m.content {
            MessageContent::Text(text) => out.push(text),
            MessageContent::Parts(parts) => responses_input_contents(parts, out),
        },
        InputItem::FunctionCall(call) => {
            out.push(&call.name);
            out.push(&call.arguments);
        }
        InputItem::FunctionCallOutput(output) => match &output.output {
            FunctionOutput::Text(text) => out.push(text),
            FunctionOutput::Content(contents) => {
                for content in contents {
                    if let FunctionOutputContent::Text(text) = content {
                        out.push(&text.text);
                    }
                }
            }
        },
        InputItem::Reasoning(reasoning) => {
            reasoning.summary.iter().for_each(|s| out.push(&s.text));
            for content in reasoning.content.iter().flatten() {
                out.push(&content.text);
            }
        }
        InputItem::CustomToolCall(call) => {
            out.push(&call.name);
            out.push(&call.input);
        }
        InputItem::CustomToolCallOutput(output) => match &output.output {
            CustomOutput::Text(text) => out.push(text),
            CustomOutput::Content(contents) => responses_input_contents(contents, out),
        },
        InputItem::McpCall(call) => {
            out.push(&call.name);
            out.push(&call.arguments);
            out.push_opt(call.output.as_ref().and_then(Option::as_ref));
        }
        InputItem::ApplyPatchCallOutput(output) => {
            out.push_opt(output.output.as_ref().and_then(Option::as_ref));
        }
        InputItem::ProgramOutput(output) => out.push(&output.result),
        // Item references, compaction blobs, computer/shell/web/file/image
        // call records and MCP bookkeeping carry ids, encrypted state or
        // screenshots rather than prompt text.
        _ => {}
    }
}

/// Text parts only; `input_image` and `input_file` carry base64 or ids.
fn responses_input_contents(parts: &[openai::responses::InputContent], out: &mut Text) {
    for part in parts {
        if let openai::responses::InputContent::Text(text) = part {
            out.push(&text.text);
        }
    }
}

// ---- OpenAI client-shaped items (guardian, native compaction) -------------

fn client_item(item: &openai::guardian::ClientResponseItem, out: &mut Text) {
    use openai::guardian::{
        AgentMessageInputContent, ClientResponseItem, ContentItem, FunctionCallOutputBody,
        FunctionCallOutputContentItem, LocalShellAction, ReasoningItemContent,
        ReasoningItemReasoningSummary, WebSearchAction,
    };
    fn output(body: &FunctionCallOutputBody, out: &mut Text) {
        match body {
            FunctionCallOutputBody::Text(text) => out.push(text),
            FunctionCallOutputBody::ContentItems(items) => {
                for item in items {
                    // Images, audio and encrypted content are not text.
                    if let FunctionCallOutputContentItem::InputText(text) = item {
                        out.push(&text.text);
                    }
                }
            }
        }
    }
    match item {
        ClientResponseItem::Message(m) => {
            for content in &m.content {
                match content {
                    ContentItem::InputText(text) => out.push(&text.text),
                    ContentItem::OutputText(text) => out.push(&text.text),
                    ContentItem::InputImage(_) | ContentItem::InputAudio(_) => {}
                }
            }
        }
        ClientResponseItem::AgentMessage(m) => {
            for content in &m.content {
                if let AgentMessageInputContent::InputText(text) = content {
                    out.push(&text.text);
                }
            }
        }
        ClientResponseItem::Reasoning(r) => {
            for summary in &r.summary {
                let ReasoningItemReasoningSummary::SummaryText(text) = summary;
                out.push(&text.text);
            }
            for content in r.content.iter().flatten().flatten() {
                match content {
                    ReasoningItemContent::ReasoningText(text) => out.push(&text.text),
                    ReasoningItemContent::Text(text) => out.push(&text.text),
                }
            }
        }
        ClientResponseItem::FunctionCall(call) => {
            out.push(&call.name);
            out.push(&call.arguments);
        }
        ClientResponseItem::FunctionCallOutput(o) => output(&o.output, out),
        ClientResponseItem::CustomToolCall(call) => {
            out.push(&call.name);
            out.push(&call.input);
        }
        ClientResponseItem::CustomToolCallOutput(o) => output(&o.output, out),
        ClientResponseItem::ToolSearchCall(call) => out.push_json(&call.arguments),
        ClientResponseItem::ToolSearchOutput(o) => o.tools.iter().for_each(|t| out.push_json(t)),
        ClientResponseItem::AdditionalTools(t) => t.tools.iter().for_each(|t| out.push_json(t)),
        ClientResponseItem::LocalShellCall(call) => {
            let LocalShellAction::Exec(exec) = &call.action;
            exec.command.iter().for_each(|arg| out.push(arg));
        }
        ClientResponseItem::WebSearchCall(call) => match &call.action {
            Some(WebSearchAction::Search(search)) => {
                out.push_opt(search.query.as_ref());
                search.queries.iter().flatten().for_each(|q| out.push(q));
            }
            Some(WebSearchAction::FindInPage(find)) => out.push_opt(find.pattern.as_ref()),
            _ => {}
        },
        // `result` is the generated image itself.
        ClientResponseItem::ImageGenerationCall(call) => {
            out.push_opt(call.revised_prompt.as_ref());
        }
        // Compaction blobs, configuration updates and unknown items carry
        // encrypted state or control fields only.
        _ => {}
    }
}

// ---- Gemini ---------------------------------------------------------------

fn gemini_request(
    system_instruction: Option<&gemini::Content>,
    contents: &[gemini::Content],
    tools: Option<&[gemini::Tool]>,
    out: &mut Text,
) {
    if let Some(system) = system_instruction {
        gemini_content(system, out);
    }
    contents.iter().for_each(|c| gemini_content(c, out));
    for declaration in tools
        .unwrap_or_default()
        .iter()
        .flat_map(|tool| tool.function_declarations.iter().flatten())
    {
        out.push(&declaration.name);
        out.push(&declaration.description);
        if let Some(parameters) = &declaration.parameters {
            out.push_json(parameters);
        }
        if let Some(parameters) = &declaration.parameters_json_schema {
            out.push_json(parameters);
        }
    }
}

fn gemini_content(content: &gemini::Content, out: &mut Text) {
    for part in content.parts.iter().flatten() {
        out.push_opt(part.text.as_ref());
        if let Some(call) = &part.function_call {
            out.push(&call.name);
            if let Some(args) = &call.args {
                out.push_json(args);
            }
        }
        if let Some(response) = &part.function_response {
            out.push(&response.name);
            out.push_json(&response.response);
            // `parts` under a function response hold inline media only.
        }
        if let Some(code) = &part.executable_code {
            out.push(&code.code);
        }
        if let Some(result) = &part.code_execution_result {
            out.push_opt(result.output.as_ref());
        }
        if let Some(call) = &part.tool_call
            && let Some(args) = &call.args
        {
            out.push_json(args);
        }
        if let Some(response) = &part.tool_response
            && let Some(value) = &response.response
        {
            out.push_json(value);
        }
        // inlineData (base64) and fileData (URIs) are never counted.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn key(operation: Operation, dialect: Dialect) -> OperationKey {
        OperationKey { operation, dialect }
    }

    const BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk";

    #[test]
    fn claude_text_comes_from_blocks_not_media() {
        let body = format!(
            r#"{{"model":"claude","max_tokens":10,
            "system":[{{"type":"text","text":"be brief"}}],
            "messages":[
              {{"role":"user","content":[
                {{"type":"text","text":"what is this"}},
                {{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"{BASE64}"}}}}
              ]}},
              {{"role":"assistant","content":[
                {{"type":"thinking","thinking":"look closely","signature":"sig"}},
                {{"type":"tool_use","id":"t1","name":"lookup","input":{{"q":"cats"}}}}
              ]}},
              {{"role":"user","content":[
                {{"type":"tool_result","tool_use_id":"t1","content":[
                  {{"type":"text","text":"a cat"}},
                  {{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"{BASE64}"}}}}
                ]}}
              ]}}
            ],
            "tools":[{{"name":"lookup","description":"find things","input_schema":{{"type":"object","properties":{{"q":{{"type":"string"}}}}}}}}]}}"#
        );
        let text = request_text(
            key(Operation::GenerateContent, Dialect::Claude),
            body.as_bytes(),
        )
        .unwrap();
        assert_eq!(
            text,
            "be brief\nwhat is this\nlook closely\nlookup\n{\"q\":\"cats\"}\na cat\nlookup\nfind things\n{\"type\":\"object\",\"properties\":{\"q\":{\"type\":\"string\"}}}"
        );
        assert!(!text.contains(BASE64));
        assert_eq!(
            request_text(
                key(Operation::CountTokens, Dialect::Claude),
                br#"{"model":"claude","messages":[{"role":"user","content":"hi"}],"system":"sys"}"#
            )
            .unwrap(),
            "sys\nhi"
        );
    }

    #[test]
    fn chat_text_comes_from_parts_not_media() {
        let body = format!(
            r#"{{"model":"m","messages":[
              {{"role":"system","content":"be brief"}},
              {{"role":"user","content":[
                {{"type":"text","text":"hi there"}},
                {{"type":"image_url","image_url":{{"url":"data:image/png;base64,{BASE64}"}}}},
                {{"type":"input_audio","input_audio":{{"data":"{BASE64}","format":"wav"}}}}
              ]}},
              {{"role":"assistant","content":"ok","tool_calls":[{{"id":"c1","type":"function","function":{{"name":"f","arguments":"{{\"a\":1}}"}}}}]}},
              {{"role":"tool","tool_call_id":"c1","content":"result"}}
            ],
            "tools":[{{"type":"function","function":{{"name":"f","description":"does f","parameters":{{"type":"object"}}}}}}],
            "tool_choice":"auto"}}"#
        );
        let text = request_text(
            key(Operation::GenerateContent, Dialect::OpenAiChat),
            body.as_bytes(),
        )
        .unwrap();
        assert_eq!(
            text,
            "be brief\nhi there\nok\nf\n{\"a\":1}\nresult\nf\ndoes f\n{\"type\":\"object\"}"
        );
        assert!(!text.contains(BASE64));
    }

    #[test]
    fn responses_text_comes_from_items_not_media() {
        let body = format!(
            r#"{{"model":"m","instructions":"be brief","input":[
              {{"type":"message","role":"user","content":[
                {{"type":"input_text","text":"hi there"}},
                {{"type":"input_image","image_url":"data:image/png;base64,{BASE64}","detail":"auto"}},
                {{"type":"input_file","file_data":"{BASE64}","filename":"a.pdf"}}
              ]}},
              {{"type":"reasoning","id":"r1","summary":[{{"type":"summary_text","text":"thinking"}}]}},
              {{"type":"function_call","call_id":"c1","name":"f","arguments":"{{\"a\":1}}"}},
              {{"type":"function_call_output","call_id":"c1","output":"result"}},
              {{"type":"message","role":"assistant","id":"m1","status":"completed","content":[{{"type":"output_text","text":"done","annotations":[],"logprobs":[]}}]}}
            ],
            "tools":[{{"type":"function","name":"f","description":"does f","parameters":{{"type":"object"}},"strict":false}}]}}"#
        );
        let text = request_text(
            key(Operation::StreamGenerateContent, Dialect::OpenAi),
            body.as_bytes(),
        )
        .unwrap();
        assert_eq!(
            text,
            "be brief\nhi there\nthinking\nf\n{\"a\":1}\nresult\ndone\nf\ndoes f\n{\"type\":\"object\"}"
        );
        assert!(!text.contains(BASE64));
        // The websocket envelope flattens the same body under a type tag.
        assert_eq!(
            request_text(
                key(
                    Operation::StreamGenerateContent,
                    Dialect::OpenAiResponsesWebSocket
                ),
                br#"{"type":"response.create","input":"plain"}"#
            )
            .unwrap(),
            "plain"
        );
        assert_eq!(
            request_text(
                key(Operation::CountTokens, Dialect::OpenAi),
                br#"{"model":"m","input":"count me","instructions":"sys"}"#
            )
            .unwrap(),
            "sys\ncount me"
        );
    }

    #[test]
    fn client_shaped_items_cover_guardian_compaction_and_memory() {
        let body = format!(
            r#"{{"model":"m","instructions":"review this","input":[
              {{"type":"message","role":"user","content":[
                {{"type":"input_text","text":"hi there"}},
                {{"type":"input_image","image_url":"data:image/png;base64,{BASE64}"}}
              ]}},
              {{"type":"function_call","name":"f","arguments":"{{\"a\":1}}","call_id":"c1"}},
              {{"type":"function_call_output","call_id":"c1","output":[{{"type":"input_text","text":"result"}}]}}
            ],"tools":[{{"type":"function","name":"f"}}],"tool_choice":"auto","parallel_tool_calls":false,
            "reasoning":null,"store":false,"stream":false,"include":[]}}"#
        );
        let expected =
            "review this\nhi there\nf\n{\"a\":1}\nresult\n[{\"name\":\"f\",\"type\":\"function\"}]";
        for operation in [Operation::GuardianReview, Operation::GuardianClassify] {
            let text = request_text(key(operation, Dialect::OpenAi), body.as_bytes()).unwrap();
            assert_eq!(text, expected);
            assert!(!text.contains(BASE64));
        }
        assert_eq!(
            request_text(
                key(Operation::CompactContent, Dialect::OpenAi),
                body.as_bytes()
            )
            .unwrap(),
            expected
        );
        assert_eq!(
            request_text(
                key(Operation::SummarizeMemory, Dialect::OpenAi),
                br#"{"model":"m","traces":[{"id":"a","metadata":{"source_path":"/tmp/a"},"items":[{"opaque":true},"a"]}]}"#
            )
            .unwrap(),
            "/tmp/a\n{\"opaque\":true}\n\"a\""
        );
    }

    #[test]
    fn openai_embeddings_count_strings_not_token_ids() {
        let key = key(Operation::CreateEmbedding, Dialect::OpenAi);
        assert_eq!(
            request_text(key, br#"{"model":"e","input":["one","two"]}"#).unwrap(),
            "one\ntwo"
        );
        assert_eq!(
            request_text(key, br#"{"model":"e","input":"one"}"#).unwrap(),
            "one"
        );
        assert_eq!(
            request_text(key, br#"{"model":"e","input":[[1,2,3]]}"#),
            None
        );
    }

    #[test]
    fn gemini_text_comes_from_parts_not_media() {
        let body = format!(
            r#"{{"systemInstruction":{{"parts":[{{"text":"be brief"}}]}},
            "contents":[
              {{"role":"user","parts":[
                {{"text":"what is this"}},
                {{"inlineData":{{"mimeType":"image/png","data":"{BASE64}"}}}},
                {{"fileData":{{"mimeType":"image/png","fileUri":"gs://bucket/{BASE64}"}}}}
              ]}},
              {{"role":"model","parts":[{{"functionCall":{{"name":"lookup","args":{{"q":"cats"}}}}}}]}},
              {{"role":"user","parts":[{{"functionResponse":{{"name":"lookup","response":{{"answer":"a cat"}}}}}}]}}
            ],
            "tools":[{{"functionDeclarations":[{{"name":"lookup","description":"find things","parameters":{{"type":"OBJECT"}}}}]}}]}}"#
        );
        let text = request_text(
            key(Operation::GenerateContent, Dialect::Gemini),
            body.as_bytes(),
        )
        .unwrap();
        assert_eq!(
            text,
            "be brief\nwhat is this\nlookup\n{\"q\":\"cats\"}\nlookup\n{\"answer\":\"a cat\"}\nlookup\nfind things\n{\"type\":\"OBJECT\"}"
        );
        assert!(!text.contains(BASE64));
        assert_eq!(
            request_text(
                key(Operation::CountTokens, Dialect::Gemini),
                br#"{"contents":[{"parts":[{"text":"count me"}]}]}"#
            )
            .unwrap(),
            "count me"
        );
        assert_eq!(
            request_text(
                key(Operation::BatchCreateEmbedding, Dialect::Gemini),
                br#"{"requests":[{"model":"e","content":{"parts":[{"text":"one"}]}},{"model":"e","content":{"parts":[{"text":"two"}]}}]}"#
            )
            .unwrap(),
            "one\ntwo"
        );
    }

    #[test]
    fn unknown_pairs_and_rejected_bodies_yield_no_input_estimate() {
        // No extractor for rerank, even though the body has obvious text.
        assert_eq!(
            request_text(
                key(Operation::Rerank, Dialect::OpenAi),
                br#"{"model":"r","query":"cats","documents":["a cat","a dog"]}"#
            ),
            None
        );
        // A Claude body missing the required `max_tokens` is not the wire
        // type; nothing is guessed from the message it carries.
        assert_eq!(
            request_text(
                key(Operation::GenerateContent, Dialect::Claude),
                br#"{"model":"claude","messages":[{"role":"user","content":"hi there"}]}"#
            ),
            None
        );
        assert_eq!(
            request_text(
                key(Operation::GenerateContent, Dialect::OpenAi),
                br#"{"q":1}"#
            ),
            None
        );
        // The output side is still estimated from the response bytes.
        let usage = Estimator::default()
            .complete(
                key(Operation::Rerank, Dialect::OpenAi),
                "p",
                Some("gpt-4o"),
                Some(br#"{"query":"cats"}"#),
                10,
                None,
            )
            .unwrap();
        assert_eq!(usage.tokens.input_tokens, None);
        assert_eq!(usage.tokens.output_tokens, Some(5));
        assert_eq!(usage.completeness, UsageCompleteness::Partial);
    }

    #[test]
    fn missing_counts_are_estimated_and_marked() {
        let estimator = Estimator::default();
        let key = key(Operation::GenerateContent, Dialect::OpenAi);
        let usage = estimator
            .complete(
                key,
                "p",
                Some("gpt-4o"),
                Some(br#"{"input":"hello world"}"#),
                10,
                None,
            )
            .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(2));
        assert_eq!(usage.tokens.output_tokens, Some(5));
        assert_eq!(usage.completeness, UsageCompleteness::Partial);
        assert_eq!(usage.dimensions["estimated"], "true");
        let reported = NormalizedUsage {
            tokens: gproxy_channel::channel::TokenUsage {
                input_tokens: Some(7),
                output_tokens: Some(3),
                ..Default::default()
            },
            ..Default::default()
        };
        let kept = estimator
            .complete(key, "p", Some("gpt-4o"), None, 100, Some(reported.clone()))
            .unwrap();
        assert_eq!(kept, reported, "reported values are never touched");
        assert!(estimator.complete(key, "p", None, None, 0, None).is_none());
    }
}
