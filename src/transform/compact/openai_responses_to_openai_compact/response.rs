use crate::protocol::openai;
use crate::transform::TransformContext;

pub fn response(
    input: openai::ResponseObject,
    _: &TransformContext,
) -> openai::CompactedResponseObject {
    openai::CompactedResponseObject {
        id: input.id,
        created_at: input.created_at,
        object: openai::ResponseCompactionObjectType::ResponseCompaction,
        output: input
            .output
            .into_iter()
            .filter_map(output_item_to_compact_item)
            .collect(),
        usage: input.usage.unwrap_or_else(default_usage),
        extra: Default::default(),
    }
}

fn output_item_to_compact_item(
    item: openai::ResponseOutputItem,
) -> Option<openai::CompactResponseItem> {
    match item.0 {
        openai::ResponseItem::Typed(typed) => Some(openai::CompactResponseItem::Typed(typed)),
        openai::ResponseItem::Unknown(unknown) => {
            Some(openai::CompactResponseItem::Unknown(unknown))
        }
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => Some(
            openai::CompactResponseItem::Message(output_message_to_compact(message)),
        ),
        // Input/EasyInput messages do not appear in a response's `output`.
        openai::ResponseItem::Message(_) => None,
    }
}

fn output_message_to_compact(
    message: openai::ResponseOutputMessageItem,
) -> openai::CompactMessageItem {
    openai::CompactMessageItem {
        id: message.id,
        type_: message.type_,
        content: message
            .content
            .into_iter()
            .map(output_part_to_compact_part)
            .collect(),
        role: openai::CompactMessageRole::Assistant,
        status: message.status,
        phase: message.phase,
        extra: Default::default(),
    }
}

fn output_part_to_compact_part(
    part: openai::ResponseMessageOutputContentPart,
) -> openai::CompactMessageContentPart {
    let part = match part {
        openai::ResponseMessageOutputContentPart::OutputText {
            annotations,
            logprobs,
            text,
            ..
        } => openai::ResponseOutputContentPart::OutputText {
            annotations,
            logprobs,
            text,
            extra: Default::default(),
        },
        openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
            openai::ResponseOutputContentPart::Refusal {
                refusal,
                extra: Default::default(),
            }
        }
    };
    openai::CompactMessageContentPart::Output(part)
}

fn default_usage() -> openai::ResponseUsage {
    openai::ResponseUsage {
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        input_tokens_details: None,
        output_tokens_details: openai::ResponseOutputTokensDetails {
            reasoning_tokens: 0,
            extra: Default::default(),
        },
        extra: Default::default(),
    }
}
