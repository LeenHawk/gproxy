use gproxy_protocol::claude::create_message::response::CreateMessageResponse as ClaudeCreateMessageResponse;
use gproxy_protocol::claude::create_message::types::{
    BetaCacheCreation, BetaContentBlock, BetaMessage, BetaMessageRole, BetaMessageType,
    BetaServiceTierUsed, BetaStopReason, BetaTextBlock, BetaTextBlockType, BetaUsage,
};
use gproxy_protocol::openai::create_response::response::Response as OpenAIResponse;
use gproxy_protocol::openai::create_response::types::{
    OutputItem, OutputMessageContent, ResponseIncompleteDetails, ResponseIncompleteReason,
    ResponseStatus,
};

/// Convert an OpenAI responses response into a Claude create-message response.
pub fn transform_response(response: OpenAIResponse) -> ClaudeCreateMessageResponse {
    let content = build_content(&response);
    let usage = build_usage(&response);
    let stop_reason = map_status(response.status, response.incomplete_details.as_ref());

    BetaMessage {
        id: response.id,
        container: None,
        content,
        context_management: None,
        model: gproxy_protocol::claude::count_tokens::types::Model::Custom(response.model),
        role: BetaMessageRole::Assistant,
        stop_reason,
        stop_sequence: None,
        r#type: BetaMessageType::Message,
        usage,
    }
}

fn build_content(response: &OpenAIResponse) -> Vec<BetaContentBlock> {
    if let Some(text) = response.output_text.as_ref()
        && !text.is_empty() {
            return vec![BetaContentBlock::Text(BetaTextBlock {
                citations: None,
                text: text.clone(),
                r#type: BetaTextBlockType::Text,
            })];
        }

    let mut combined = String::new();
    for item in &response.output {
        if let OutputItem::Message(message) = item {
            for part in &message.content {
                match part {
                    OutputMessageContent::OutputText(text) => combined.push_str(&text.text),
                    OutputMessageContent::Refusal(refusal) => combined.push_str(&refusal.refusal),
                }
            }
        }
    }

    if combined.is_empty() {
        Vec::new()
    } else {
        vec![BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text: combined,
            r#type: BetaTextBlockType::Text,
        })]
    }
}

fn build_usage(response: &OpenAIResponse) -> BetaUsage {
    let (input_tokens, output_tokens) = response
        .usage
        .as_ref()
        .map(|usage| (usage.input_tokens.max(0) as u32, usage.output_tokens.max(0) as u32))
        .unwrap_or((0, 0));
    BetaUsage {
        cache_creation: BetaCacheCreation {
            ephemeral_1h_input_tokens: 0,
            ephemeral_5m_input_tokens: 0,
        },
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        input_tokens,
        output_tokens,
        server_tool_use: None,
        service_tier: BetaServiceTierUsed::Standard,
    }
}

fn map_status(
    status: Option<ResponseStatus>,
    details: Option<&ResponseIncompleteDetails>,
) -> Option<BetaStopReason> {
    match status {
        Some(ResponseStatus::Completed) => Some(BetaStopReason::EndTurn),
        Some(ResponseStatus::Incomplete) => match details.map(|d| d.reason) {
            Some(ResponseIncompleteReason::MaxOutputTokens) => Some(BetaStopReason::MaxTokens),
            Some(ResponseIncompleteReason::ContentFilter) => Some(BetaStopReason::Refusal),
            None => Some(BetaStopReason::PauseTurn),
        },
        Some(ResponseStatus::Failed) | Some(ResponseStatus::Cancelled) => {
            Some(BetaStopReason::PauseTurn)
        }
        Some(ResponseStatus::InProgress) | Some(ResponseStatus::Queued) | None => None,
    }
}
