use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::openai::common::OpenAiModelId;
use gproxy_protocol::openai::generate_content::responses::{
    ResponseCreateRequest, ResponseEasyInputContent, ResponseEasyInputMessageItem,
    ResponseEasyInputMessageRole, ResponseInput, ResponseInputContentPart,
    ResponseInputMessageRole, ResponseItem, ResponseMessageItem, ResponseMessageItemType,
    TypedResponseItem,
};

pub(super) fn request(body: &Bytes, model: &str) -> Result<Bytes, ChannelError> {
    let mut request: ResponseCreateRequest = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Responses request JSON: {error}")))?;
    request.model = Some(OpenAiModelId::from(model));
    request.stream = Some(true);
    request.store = Some(false);
    request.max_output_tokens = None;
    request.metadata = None;
    request.prompt_cache_options = None;
    request.temperature = None;
    request.top_p = None;
    request.top_logprobs = None;
    request.safety_identifier = None;
    request.truncation = None;

    let mut instructions = request.instructions.take();
    if let Some(input) = request.input.take() {
        request.input = Some(match input {
            ResponseInput::Text(text) => ResponseInput::Items(vec![ResponseItem::Message(
                ResponseMessageItem::EasyInput(ResponseEasyInputMessageItem {
                    type_: Some(ResponseMessageItemType::Message),
                    role: ResponseEasyInputMessageRole::User,
                    content: ResponseEasyInputContent::Text(text),
                    phase: None,
                    rest: Default::default(),
                }),
            )]),
            input => input,
        });
    }
    if let Some(ResponseInput::Items(items)) = request.input.as_mut() {
        let mut retained = Vec::with_capacity(items.len());
        for mut item in std::mem::take(items) {
            if let Some(text) = system_text(&item) {
                append_instruction(&mut instructions, text);
            } else {
                strip_replay_status(&mut item);
                retained.push(item);
            }
        }
        super::tools::normalize_history(&mut retained)?;
        *items = retained;
    }
    super::tools::normalize_definitions(&mut request.tools, &mut request.tool_choice);
    request.instructions = instructions;
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn system_text(item: &ResponseItem) -> Option<String> {
    let message = match item {
        ResponseItem::Message(message) => message,
        _ => return None,
    };
    let mut parts = Vec::new();
    match message {
        ResponseMessageItem::Input(message) if message.role == ResponseInputMessageRole::System => {
            collect_parts(&message.content, &mut parts)
        }
        ResponseMessageItem::EasyInput(message)
            if message.role == ResponseEasyInputMessageRole::System =>
        {
            match &message.content {
                ResponseEasyInputContent::Text(text) => parts.push(text.clone()),
                ResponseEasyInputContent::Parts(content) => collect_parts(content, &mut parts),
                ResponseEasyInputContent::OutputParts(_) | ResponseEasyInputContent::Unknown(_) => {
                }
            }
        }
        _ => return None,
    }
    Some(parts.join("\n"))
}

fn collect_parts(parts: &[ResponseInputContentPart], output: &mut Vec<String>) {
    for part in parts {
        if let ResponseInputContentPart::InputText(text) = part {
            output.push(text.text.clone());
        }
    }
}

fn append_instruction(instructions: &mut Option<String>, text: String) {
    if text.is_empty() {
        return;
    }
    let instructions = instructions.get_or_insert_with(String::new);
    if !instructions.is_empty() {
        instructions.push('\n');
    }
    instructions.push_str(&text);
}

fn strip_replay_status(item: &mut ResponseItem) {
    if let ResponseItem::Typed(item) = item
        && let TypedResponseItem::Reasoning { status, .. } = item.as_mut()
    {
        *status = None;
    }
}
