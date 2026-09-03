use gproxy_protocol::openai;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input = serde_json::from_slice(&body)?;
    let output = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: gproxy_protocol::claude::CreateMessageResponseBody,
) -> Result<openai::CompactedResponseObject, TransformError> {
    let response =
        crate::generate_content::openai_responses_to_claude_messages::response::transform_typed(
            input,
        )?;
    from_responses_typed(response)
}

pub(crate) fn from_responses_typed(
    input: openai::ResponseObject,
) -> Result<openai::CompactedResponseObject, TransformError> {
    let usage = input.usage.unwrap_or_else(empty_usage);
    let incomplete = input.status == Some(openai::ResponseStatus::Incomplete);
    let mut items = input
        .output
        .into_iter()
        .filter_map(|item| compact_item(item, incomplete).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    items.sort_by_key(|item| !matches!(item, openai::CompactResponseItem::Message(_)));
    let output = crate::wire!(openai::CompactedResponseObject {
        id: input.id,
        created_at: input.created_at,
        object: openai::ResponseCompactionObjectType::ResponseCompaction,
        output: items,
        usage,
        rest: Default::default(),
    });
    Ok(output)
}

fn compact_item(
    item: openai::ResponseItem,
    incomplete: bool,
) -> Result<Option<openai::CompactResponseItem>, TransformError> {
    Ok(match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => Some(
            openai::CompactResponseItem::Message(crate::wire!(openai::CompactMessageItem {
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
                        #[cfg(not(feature = "exhaustive"))]
                        _ => None,
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
            })),
        ),
        openai::ResponseItem::Typed(item) => Some(openai::CompactResponseItem::Typed(item)),
        openai::ResponseItem::Unknown(_) | openai::ResponseItem::Message(_) => None,
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}

fn empty_usage() -> openai::ResponseUsage {
    crate::wire!(openai::ResponseUsage {
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        input_tokens_details: None,
        output_tokens_details: None,
        rest: Default::default(),
    })
}
