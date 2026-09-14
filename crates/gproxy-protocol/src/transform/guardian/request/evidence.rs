use super::*;

pub(super) fn task_payload(
    input: &source::GuardianRequestBody,
    operation: GuardianOperation,
    limits: CodecLimits,
) -> Result<String, TransformError> {
    let mut attachment_index = 0;
    let history = input
        .input
        .iter()
        .map(|v| item(v, &mut attachment_index))
        .collect::<Result<Vec<_>, _>>()?;
    let (operation, task) = match operation {
        GuardianOperation::Review => (
            "review",
            "Review the declared history. Return strict JSON matching the Guardian review contract.",
        ),
        GuardianOperation::Classify => (
            "classify",
            "Classify the declared history. Your complete answer must be exactly one lowercase token: high or low.",
        ),
    };
    let value = json!({
        "operation": operation,
        "history": history,
        "declared_tools": input.tools,
        "tool_choice": input.tool_choice,
        "parallel_tool_calls": input.parallel_tool_calls,
        "task": task,
    });
    let bytes = codec::encode_json(&value, limits).map_err(|e| {
        if e.kind() == crate::codec::CodecErrorKind::Limit {
            TransformError::new(
                crate::transform::TransformErrorKind::Limit,
                "guardian.history",
                e.to_string(),
            )
        } else {
            TransformError::shape("guardian.history", e.to_string())
        }
    })?;
    String::from_utf8(bytes.to_vec())
        .map_err(|e| TransformError::shape("guardian.history", e.to_string()))
}

pub(super) fn attachment(index: &mut usize) -> String {
    let id = format!("attachment:{index}");
    *index += 1;
    id
}

pub(super) fn content_item(
    value: &source::ContentItem,
    index: &mut usize,
) -> Result<Value, TransformError> {
    match value {
        source::ContentItem::InputText(v) => Ok(json!({"type":"input_text","text":v.text})),
        source::ContentItem::OutputText(v) => Ok(json!({"type":"output_text","text":v.text})),
        source::ContentItem::InputImage(v) => {
            Ok(json!({"type":"input_image","attachment": attachment(index),"detail":v.detail}))
        }
        source::ContentItem::InputAudio(_) => {
            Ok(json!({"type":"input_audio","attachment": attachment(index)}))
        }
    }
}

pub(super) fn output_body(
    value: &source::FunctionCallOutputBody,
    index: &mut usize,
) -> Result<Value, TransformError> {
    match value {
        source::FunctionCallOutputBody::Text(text) => Ok(json!(text)),
        source::FunctionCallOutputBody::ContentItems(items) => Ok(Value::Array(
            items
                .iter()
                .map(|item| match item {
                    source::FunctionCallOutputContentItem::InputText(v) => {
                        Ok(json!({"type":"input_text","text":v.text}))
                    }
                    source::FunctionCallOutputContentItem::InputImage(v) => {
                        Ok(json!({"type":"input_image","attachment": attachment(index),"detail":v.detail}))
                    }
                    source::FunctionCallOutputContentItem::InputAudio(_) => {
                        Ok(json!({"type":"input_audio","attachment": attachment(index)}))
                    }
                    source::FunctionCallOutputContentItem::EncryptedContent(_) => {
                        Err(TransformError::unsupported(
                            "guardian.tool_output.encrypted_content",
                            "encrypted content cannot be interpreted by this adapter",
                        ))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
    }
}

pub(super) fn reasoning_content(
    value: Option<&Option<Vec<source::ReasoningItemContent>>>,
) -> Value {
    match value.and_then(Option::as_ref) {
        None => Value::Null,
        Some(items) => Value::Array(
            items
                .iter()
                .map(|item| match item {
                    source::ReasoningItemContent::ReasoningText(v) => {
                        json!({"type":"reasoning_text","text":v.text})
                    }
                    source::ReasoningItemContent::Text(v) => json!({"type":"text","text":v.text}),
                })
                .collect(),
        ),
    }
}

pub(super) fn local_action(value: &source::LocalShellAction) -> Value {
    match value {
        source::LocalShellAction::Exec(v) => {
            json!({"type":"exec","command":v.command,"timeout_ms":v.timeout_ms,"working_directory":v.working_directory,"env":v.env,"user":v.user})
        }
    }
}

pub(super) fn web_action(value: &Option<source::WebSearchAction>) -> Value {
    match value {
        None => Value::Null,
        Some(source::WebSearchAction::Search(v)) => {
            json!({"type":"search","query":v.query,"queries":v.queries})
        }
        Some(source::WebSearchAction::OpenPage(v)) => json!({"type":"open_page","url":v.url}),
        Some(source::WebSearchAction::FindInPage(v)) => {
            json!({"type":"find_in_page","url":v.url,"pattern":v.pattern})
        }
        Some(source::WebSearchAction::Other(_)) => json!({"type":"other"}),
    }
}

pub(super) fn item(
    value: &source::ClientResponseItem,
    index: &mut usize,
) -> Result<Value, TransformError> {
    let value = match value {
        source::ClientResponseItem::Message(v) => {
            json!({"type":"message","id":v.id,"role":v.role,"phase":v.phase,"internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"content":v.content.iter().map(|v| content_item(v, index)).collect::<Result<Vec<_>, _>>()?})
        }
        source::ClientResponseItem::AgentMessage(v) => {
            json!({"type":"agent_message","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"author":v.author,"recipient":v.recipient,"content":v.content.iter().map(|part| match part { source::AgentMessageInputContent::InputText(x) => Ok(json!({"type":"input_text","text":x.text})), source::AgentMessageInputContent::EncryptedContent(_) => Err(TransformError::unsupported("guardian.agent_message.encrypted_content", "encrypted content cannot be interpreted by this adapter")) }).collect::<Result<Vec<_>, _>>()?})
        }
        source::ClientResponseItem::Reasoning(v) => {
            if v.encrypted_content.is_some()
                && !v.summary.iter().any(|v| match v {
                    source::ReasoningItemReasoningSummary::SummaryText(v) => {
                        !v.text.trim().is_empty()
                    }
                })
                && !v
                    .content
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|v| {
                        v.iter().any(|v| match v {
                            source::ReasoningItemContent::ReasoningText(v) => {
                                !v.text.trim().is_empty()
                            }
                            source::ReasoningItemContent::Text(v) => !v.text.trim().is_empty(),
                        })
                    })
            {
                return Err(TransformError::unsupported(
                    "guardian.reasoning.encrypted_content",
                    "encrypted reasoning requires resolved context",
                ));
            }
            json!({"type":"reasoning","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"summary":v.summary.iter().map(|part| match part { source::ReasoningItemReasoningSummary::SummaryText(x) => json!(x.text) }).collect::<Vec<_>>(),"content":reasoning_content(v.content.as_ref()),"encrypted_content":Value::Null})
        }
        source::ClientResponseItem::FunctionCall(v) => {
            if v.encrypted_function_args
                .as_ref()
                .is_some_and(|v| !v.is_empty())
            {
                return Err(TransformError::unsupported(
                    "guardian.function_call.encrypted_function_args",
                    "encrypted tool arguments require resolved context",
                ));
            }
            json!({"type":"function_call","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"name":v.name,"namespace":v.namespace,"arguments":v.arguments,"call_id":v.call_id})
        }
        source::ClientResponseItem::FunctionCallOutput(v) => {
            json!({"type":"function_call_output","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"name":v.name,"namespace":v.namespace,"call_id":v.call_id,"output":output_body(&v.output, index)?})
        }
        source::ClientResponseItem::CustomToolCall(v) => {
            json!({"type":"custom_tool_call","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"status":v.status,"call_id":v.call_id,"name":v.name,"namespace":v.namespace,"input":v.input})
        }
        source::ClientResponseItem::CustomToolCallOutput(v) => {
            json!({"type":"custom_tool_call_output","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"call_id":v.call_id,"name":v.name,"output":output_body(&v.output, index)?})
        }
        source::ClientResponseItem::ToolSearchCall(v) => {
            json!({"type":"tool_search_call","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"call_id":v.call_id,"status":v.status,"execution":v.execution,"arguments":v.arguments})
        }
        source::ClientResponseItem::ToolSearchOutput(v) => {
            json!({"type":"tool_search_output","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"call_id":v.call_id,"status":v.status,"execution":v.execution,"tools":v.tools})
        }
        source::ClientResponseItem::AdditionalTools(v) => {
            json!({"type":"additional_tools","id":v.id,"role":v.role,"tools":v.tools})
        }
        source::ClientResponseItem::LocalShellCall(v) => {
            json!({"type":"local_shell_call","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"call_id":v.call_id,"status":v.status,"action":local_action(&v.action)})
        }
        source::ClientResponseItem::WebSearchCall(v) => {
            json!({"type":"web_search_call","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"status":v.status,"action":web_action(&v.action)})
        }
        source::ClientResponseItem::ImageGenerationCall(v) => {
            json!({"type":"image_generation_call","internal_chat_message_metadata_passthrough":v.internal_chat_message_metadata_passthrough,"id":v.id,"status":v.status,"revised_prompt":v.revised_prompt,"result":attachment(index)})
        }
        source::ClientResponseItem::Compaction(_) => {
            return Err(TransformError::unsupported(
                "guardian.compaction.encrypted_content",
                "encrypted compaction requires resolved context",
            ));
        }
        source::ClientResponseItem::ConfigurationUpdate(v) => {
            json!({"type":"configuration_update","reasoning":{"effort":v.reasoning.effort}})
        }
        source::ClientResponseItem::CompactionTrigger(_) => json!({"type":"compaction_trigger"}),
        source::ClientResponseItem::ContextCompaction(_) => {
            return Err(TransformError::unsupported(
                "guardian.context_compaction.encrypted_content",
                "encrypted compaction requires resolved context",
            ));
        }
        source::ClientResponseItem::Other(_) => {
            return Err(TransformError::unsupported(
                "guardian.input",
                "opaque history item has no declared Guardian mapping",
            ));
        }
    };
    Ok(value)
}
