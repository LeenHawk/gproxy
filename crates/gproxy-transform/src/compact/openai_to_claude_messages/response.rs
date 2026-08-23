use gproxy_protocol::openai;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let converted =
        crate::generate_content::openai_responses_to_claude_messages::response::claude_to_responses(
            body,
        )?;
    let input: openai::ResponseObject = serde_json::from_slice(&converted)?;
    let usage = input
        .usage
        .ok_or_else(|| TransformError::shape("Claude compact response", "usage is incomplete"))?;
    let output = openai::CompactedResponseObject {
        id: input.id,
        created_at: input.created_at,
        object: openai::ResponseCompactionObjectType::ResponseCompaction,
        output: input
            .output
            .into_iter()
            .map(compact_item)
            .collect::<Result<_, _>>()?,
        usage,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn compact_item(item: openai::ResponseItem) -> Result<openai::CompactResponseItem, TransformError> {
    Ok(match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            openai::CompactResponseItem::Message(openai::CompactMessageItem {
                id: message.id,
                type_: message.type_,
                content: message
                    .content
                    .into_iter()
                    .map(|part| match part {
                        openai::ResponseMessageOutputContentPart::OutputText(part) => {
                            openai::CompactMessageContentPart::Text(openai::CompactTextContent {
                                text: part.text,
                                type_: openai::CompactTextContentType::Text,
                                rest: part.rest,
                            })
                        }
                        openai::ResponseMessageOutputContentPart::Refusal(part) => {
                            openai::CompactMessageContentPart::Text(openai::CompactTextContent {
                                text: part.refusal,
                                type_: openai::CompactTextContentType::Text,
                                rest: part.rest,
                            })
                        }
                        openai::ResponseMessageOutputContentPart::Unknown(raw) => {
                            openai::CompactMessageContentPart::Unknown(raw)
                        }
                    })
                    .collect(),
                role: openai::CompactMessageRole::Assistant,
                status: message.status,
                phase: message.phase,
                rest: message.rest,
            })
        }
        openai::ResponseItem::Typed(item) => openai::CompactResponseItem::Typed(item),
        openai::ResponseItem::Unknown(raw) => openai::CompactResponseItem::Unknown(raw),
        other => openai::CompactResponseItem::Unknown(serde_json::to_value(other)?),
    })
}
