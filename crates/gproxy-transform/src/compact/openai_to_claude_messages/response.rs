use gproxy_protocol::openai;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let converted =
        crate::generate_content::openai_responses_to_claude_messages::response::claude_to_responses(
            body,
        )?;
    from_responses(converted)
}

pub(crate) fn from_responses(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseObject = serde_json::from_slice(&body)?;
    let usage = input.usage.unwrap_or_else(empty_usage);
    let incomplete = input.status == Some(openai::ResponseStatus::Incomplete);
    let mut items = input
        .output
        .into_iter()
        .filter_map(|item| compact_item(item, incomplete).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    items.sort_by_key(|item| !matches!(item, openai::CompactResponseItem::Message(_)));
    let output = openai::CompactedResponseObject {
        id: input.id,
        created_at: input.created_at,
        object: openai::ResponseCompactionObjectType::ResponseCompaction,
        output: items,
        usage,
        rest: Default::default(),
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn compact_item(
    item: openai::ResponseItem,
    incomplete: bool,
) -> Result<Option<openai::CompactResponseItem>, TransformError> {
    Ok(match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => Some(
            openai::CompactResponseItem::Message(openai::CompactMessageItem {
                id: Some(message.id),
                type_: message.type_,
                content: message
                    .content
                    .into_iter()
                    .filter_map(|part| match part {
                        openai::ResponseMessageOutputContentPart::OutputText(part) => Some(
                            openai::CompactMessageContentPart::Text(openai::CompactTextContent {
                                text: part.text,
                                type_: openai::CompactTextContentType::Text,
                                rest: Default::default(),
                            }),
                        ),
                        openai::ResponseMessageOutputContentPart::Refusal(part) => Some(
                            openai::CompactMessageContentPart::Text(openai::CompactTextContent {
                                text: part.refusal,
                                type_: openai::CompactTextContentType::Text,
                                rest: Default::default(),
                            }),
                        ),
                        openai::ResponseMessageOutputContentPart::Unknown(_) => None,
                    })
                    .collect(),
                role: openai::CompactMessageRole::Assistant,
                status: if incomplete {
                    openai::ResponseItemLifecycleStatus::Incomplete
                } else {
                    message.status
                },
                phase: message.phase,
                rest: Default::default(),
            }),
        ),
        openai::ResponseItem::Typed(item) => Some(openai::CompactResponseItem::Typed(item)),
        openai::ResponseItem::Unknown(_) | openai::ResponseItem::Message(_) => None,
    })
}

fn empty_usage() -> openai::ResponseUsage {
    openai::ResponseUsage {
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        input_tokens_details: None,
        output_tokens_details: None,
        rest: Default::default(),
    }
}
