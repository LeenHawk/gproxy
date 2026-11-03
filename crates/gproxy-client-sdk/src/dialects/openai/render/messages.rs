use super::*;
pub(super) enum Pending {
    Input {
        role: i::MessageRole,
        parts: Vec<i::InputContent>,
    },
    Output {
        group: Option<u32>,
        message: i::ResponseOutputMessage,
    },
}

pub(super) fn flush(pending: Option<Pending>, input: &mut Vec<i::InputItem>) {
    if let Some(pending) = pending {
        input.push(match pending {
            Pending::Input { role, parts } => i::InputItem::Easy(
                i::EasyInputMessage::builder(i::MessageContent::Parts(parts), role)
                    .type_(i::MessageType::Message)
                    .build(),
            ),
            Pending::Output { message, .. } => i::InputItem::OutputMessage(message),
        });
    }
}

pub(super) fn push_input(
    pending: &mut Option<Pending>,
    input: &mut Vec<i::InputItem>,
    role: i::MessageRole,
    part: i::InputContent,
) {
    match pending.take() {
        Some(Pending::Input { role, mut parts }) => {
            parts.push(part);
            *pending = Some(Pending::Input { role, parts });
        }
        other => {
            flush(other, input);
            *pending = Some(Pending::Input {
                role,
                parts: vec![part],
            });
        }
    }
}

pub(super) fn push_output(
    pending: &mut Option<Pending>,
    input: &mut Vec<i::InputItem>,
    meta: PartMetadata,
    part: i::OutputContent,
) -> Result<()> {
    match pending.take() {
        Some(Pending::Output { group, mut message }) if group == meta.group => {
            message.content.push(part);
            *pending = Some(Pending::Output { group, message });
        }
        other => {
            flush(other, input);
            let item = meta.item.ok_or_else(|| {
                Error::invalid("native output-text metadata requires its message envelope")
            })?;
            let mut message = i::ResponseOutputMessage::builder(
                item.id,
                Vec::new(),
                i::OutputMessageRole::Assistant,
                item.status,
                i::MessageType::Message,
            )
            .rest(item.rest)
            .build();
            message.agent = item.agent;
            message.phase = item.phase;
            message.content = vec![part];
            *pending = Some(Pending::Output {
                group: meta.group,
                message,
            });
        }
    }
    Ok(())
}

pub(super) fn message_role(role: &Role) -> Result<i::MessageRole> {
    Ok(match role {
        Role::System => i::MessageRole::System,
        Role::User => i::MessageRole::User,
        Role::Assistant => i::MessageRole::Assistant,
        Role::Unknown(_) => return Err(Error::invalid("unknown persisted message role")),
        Role::Tool => {
            return Err(Error::invalid(
                "Responses tool messages require tool-result blocks",
            ));
        }
    })
}

pub(super) fn render_messages(messages: &[Message]) -> Result<Vec<i::InputItem>> {
    let mut input = Vec::new();
    for message in messages {
        if matches!(&message.role, Role::Unknown(_)) {
            continue;
        }
        let mut pending = None;
        for block in &message.content {
            match &block.content {
                Content::Text(text) => {
                    render_text(block, text, &message.role, &mut pending, &mut input)?;
                }
                Content::Image(image) => {
                    if message.role == Role::Assistant
                        && let Some(generated) = generated_image(block, image)?
                    {
                        flush(pending.take(), &mut input);
                        input.push(generated);
                    } else {
                        push_input(
                            &mut pending,
                            &mut input,
                            message_role(&message.role)?,
                            i::InputContent::Image(input_image(block, image)?),
                        );
                    }
                }
                Content::Document(document) => push_input(
                    &mut pending,
                    &mut input,
                    message_role(&message.role)?,
                    i::InputContent::File(input_document(block, document)?),
                ),
                Content::File(file) => {
                    media::check_file_vendor(&file.source, Vendor::Openai)?;
                    let source = &file.source;
                    let value = input_file(
                        block,
                        source,
                        file.media_type.as_deref(),
                        file.filename.as_deref(),
                        None,
                    )?;
                    push_input(
                        &mut pending,
                        &mut input,
                        message_role(&message.role)?,
                        i::InputContent::File(value),
                    );
                }
                Content::Reasoning(Reasoning { text }) => {
                    flush(pending.take(), &mut input);
                    if let Some(meta) = read_native::<ReasoningMetadata>(block, Vendor::Openai)? {
                        let mut chars = text.chars();
                        let count = meta.summary.len();
                        let mut summary = Vec::with_capacity(count);
                        for (index, part) in meta.summary.into_iter().enumerate() {
                            let text = if index + 1 == count {
                                chars.by_ref().collect()
                            } else {
                                chars.by_ref().take(part.chars).collect()
                            };
                            summary.push(
                                i::SummaryText::builder(i::SummaryTextType::SummaryText, text)
                                    .rest(part.rest)
                                    .build(),
                            );
                        }
                        let mut value = i::ReasoningItem::builder(
                            i::ReasoningItemType::ReasoningItem,
                            meta.id,
                            summary,
                        )
                        .rest(meta.rest)
                        .build();
                        value.content = meta.content;
                        value.status = meta.status;
                        value.agent = meta.agent;
                        value.encrypted_content = meta.encrypted_content;
                        input.push(i::InputItem::Reasoning(value));
                    }
                }
                Content::ToolCall(call) => {
                    flush(pending.take(), &mut input);
                    if call.provider_executed {
                        if let Some(value) = restore_server(block, call)? {
                            input.push(value);
                        }
                        continue;
                    }
                    let mut value = i::FunctionCall::builder(
                        i::FunctionCallType::FunctionCall,
                        call.arguments.clone(),
                        call.id.clone(),
                        call.name.clone(),
                    )
                    .build();
                    if let Some(meta) = read_native::<FunctionMetadata>(block, Vendor::Openai)? {
                        value.id = meta.id;
                        value.namespace = meta.namespace;
                        value.status = meta.status;
                        value.async_ = meta.async_;
                        value.agent = meta.agent;
                        value.caller = meta.caller;
                        value.rest = meta.rest;
                    }
                    input.push(i::InputItem::FunctionCall(value));
                }
                Content::ToolResult(result) => {
                    flush(pending.take(), &mut input);
                    if result.provider_executed {
                        restore_server_output(block, result, &mut input)?;
                        continue;
                    }
                    let output = match &result.output {
                        ToolOutput::Unknown(_) => {
                            return Err(Error::invalid("unknown persisted tool output"));
                        }
                        ToolOutput::Json(Value::String(text)) => {
                            i::FunctionOutput::Text(text.clone())
                        }
                        ToolOutput::Json(value) => i::FunctionOutput::Text(value.to_string()),
                        ToolOutput::Blocks(blocks) => i::FunctionOutput::Content(
                            blocks.iter().map(function_output).collect::<Result<_>>()?,
                        ),
                    };
                    let mut value = i::FunctionCallOutput::builder(
                        i::FunctionCallOutputType::FunctionCallOutput,
                        output,
                    )
                    .build();
                    let meta = read_native::<ResultMetadata>(block, Vendor::Openai)?;
                    let include_id =
                        meta.as_ref().is_some_and(|m| m.call_id_present) || !result.id.is_empty();
                    let include_name =
                        meta.as_ref().is_some_and(|m| m.name_present) || !result.name.is_empty();
                    if let Some(meta) = meta {
                        value.id = Some(Some(meta.id));
                        value.status = Some(Some(meta.status));
                        value.namespace = meta.namespace.map(Some);
                        value.agent = meta.agent;
                        value.caller = meta.caller;
                        value.rest = meta.rest;
                        if let Some(created_by) = meta.created_by {
                            #[derive(Serialize)]
                            struct CreatedBy {
                                created_by: String,
                            }
                            value.rest.extend(encode_rest(CreatedBy { created_by })?);
                        }
                    }
                    if include_id {
                        value.call_id = Some(Some(result.id.clone()));
                    }
                    if include_name {
                        value.name = Some(Some(result.name.clone()));
                    }
                    input.push(i::InputItem::FunctionCallOutput(value));
                }
                Content::Unknown(_) => {}
                Content::Opaque => {
                    flush(pending.take(), &mut input);
                    if let Some(value) = read_native(block, Vendor::Openai)? {
                        input.push(value);
                    }
                }
                Content::Audio(_) | Content::Video(_) => {
                    return Err(Error::invalid(
                        "Responses does not accept audio/video blocks; use Chat Completions or Realtime for audio",
                    ));
                }
            }
        }
        flush(pending, &mut input);
    }
    Ok(input)
}
