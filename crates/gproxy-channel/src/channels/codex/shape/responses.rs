use super::{invalid, tools};
use gproxy_protocol::{connection::Bytes, wire::openai::responses::*};
use gproxy_upstream::UpstreamError;

pub(super) fn request(
    body: &Bytes,
    headers: &http::HeaderMap,
) -> Result<(Bytes, tools::Aliases), UpstreamError> {
    let mut request: GenerateContentRequestBody = serde_json::from_slice(body).map_err(invalid)?;
    request.stream = Some(Some(true));
    request.store = Some(Some(false));
    request.max_output_tokens = None;
    request.metadata = None;
    request.prompt_cache_options = request
        .prompt_cache_options
        .take()
        .and_then(cli_prompt_cache_options);
    request.temperature = None;
    request.top_p = None;
    request.top_logprobs = None;
    request.safety_identifier = None;
    request.user = None;
    request.truncation = None;
    // The CLI's Guardian reviewer uses backend defaults, not generation tiers.
    if headers
        .get("x-codex-guardian")
        .is_some_and(|v| v == "reviewer")
    {
        request.service_tier = None;
    }
    if let Some(Input::Text(text)) = request
        .input
        .take_if(|input| matches!(input, Input::Text(_)))
    {
        request.input = Some(Input::Items(vec![InputItem::Easy(EasyInputMessage {
            agent: None,
            type_: Some(MessageType::Message),
            role: MessageRole::User,
            content: MessageContent::Text(text),
            phase: None,
            rest: Default::default(),
        })]));
    }
    // The CLI sends no top-level `instructions`: its base instructions lead
    // `input` as a developer message (openai/codex c9253c4977). Models starting
    // with GPT-6.2 append `instructions` after `input`, where it is never cached.
    if let Some(Some(text)) = request.instructions.take()
        && !text.is_empty()
    {
        let message = InputItem::Easy(EasyInputMessage {
            agent: None,
            type_: Some(MessageType::Message),
            role: MessageRole::Developer,
            content: MessageContent::Text(text),
            phase: None,
            rest: Default::default(),
        });
        match request
            .input
            .get_or_insert_with(|| Input::Items(Vec::new()))
        {
            Input::Items(items) => items.insert(0, message),
            Input::Text(_) => unreachable!("text input became a user message above"),
        }
    }
    let mut aliases = tools::normalize_definitions(&mut request.tools, &mut request.tool_choice)?;
    if let Some(Input::Items(items)) = request.input.as_mut() {
        let mut retained = Vec::with_capacity(items.len());
        for mut item in std::mem::take(items) {
            developer_role(&mut item);
            if unresolvable(&item) {
                continue;
            }
            if let InputItem::Reasoning(reasoning) = &mut item {
                reasoning.status = None;
                reasoning.content = None;
            }
            retained.push(item);
        }
        tools::normalize_history(&mut retained, &mut aliases)?;
        *items = retained;
    }
    // Match the CLI's routing prefix so the backend can route before reading
    // a potentially large input. Keep this ordering specific to Codex.
    #[derive(serde::Serialize)]
    struct RoutedRequest<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stream: Option<Option<bool>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        service_tier: Option<Option<ServiceTier>>,
        #[serde(flatten)]
        request: &'a GenerateContentRequestBody,
    }
    let model = request.model.take();
    let stream = request.stream.take();
    let service_tier = request.service_tier.take();
    let bytes = serde_json::to_vec(&RoutedRequest {
        model,
        stream,
        service_tier,
        request: &request,
    })
    .map_err(invalid)?;
    Ok((Bytes::from(bytes), aliases))
}

// The backend forces `store=false` and persists nothing, so an item that only
// names a prior output by id cannot resolve and fails the whole request.
// Clients that assume the public API's default `store=true` send these: a bare
// `item_reference`, or a reasoning item without its `encrypted_content`.
fn unresolvable(item: &InputItem) -> bool {
    match item {
        InputItem::ItemReference(_) => true,
        InputItem::Reasoning(reasoning) => !matches!(
            &reasoning.encrypted_content,
            Some(Some(content)) if !content.is_empty()
        ),
        _ => false,
    }
}

// The backend rejects the system role; developer is its instruction role, in
// place so the message keeps its position and cache breakpoints.
fn developer_role(item: &mut InputItem) {
    match item {
        InputItem::Message(message) if message.role == InputMessageRole::System => {
            message.role = InputMessageRole::Developer;
        }
        InputItem::Easy(message) if message.role == MessageRole::System => {
            message.role = MessageRole::Developer;
        }
        _ => {}
    }
}

/// `prompt_cache_options` as the CLI sends it: only `ttl`, the minimum
/// lifetime of new cache entries (codex-api `PromptCacheOptions`, openai/codex
/// 41cbd5b9b2), on HTTP and WebSocket requests alike. The public API's mode,
/// prewarm and comparison options are not part of the backend's CLI wire;
/// without a `ttl` the field is left out. Block breakpoints stay as sent.
fn cli_prompt_cache_options(options: PromptCacheOptions) -> Option<PromptCacheOptions> {
    Some(PromptCacheOptions {
        comparison_response_id: None,
        prewarm: None,
        mode: None,
        ttl: Some(options.ttl?),
        rest: Default::default(),
    })
}
