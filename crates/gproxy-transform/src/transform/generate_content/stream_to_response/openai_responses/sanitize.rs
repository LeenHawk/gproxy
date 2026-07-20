use crate::protocol::openai;

pub(super) fn sanitize_item(item: openai::ResponseItem) -> openai::ResponseItem {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                openai::ResponseOutputMessageItem {
                    type_: message.type_,
                    id: message.id,
                    role: message.role,
                    content: message
                        .content
                        .into_iter()
                        .map(sanitize_message_content_part)
                        .collect(),
                    status: message.status,
                    phase: message.phase,
                    extra: Default::default(),
                },
            ))
        }
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            caller: None,
            namespace,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            caller: None,
            namespace,
            status,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            namespace,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            caller: None,
            namespace,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            id,
            summary,
            content,
            encrypted_content,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            id,
            summary: summary
                .into_iter()
                .map(|part| openai::ResponseReasoningSummaryPart {
                    text: part.text,
                    type_: part.type_,
                    extra: Default::default(),
                })
                .collect(),
            content: content.map(|parts| {
                parts
                    .into_iter()
                    .map(|part| openai::ResponseReasoningTextPart {
                        text: part.text,
                        type_: part.type_,
                        extra: Default::default(),
                    })
                    .collect()
            }),
            encrypted_content,
            status,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            outputs,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            outputs,
            status,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            server_label,
            approval_request_id,
            error,
            output,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            server_label,
            approval_request_id,
            error,
            output,
            status,
            extra: Default::default(),
        }),
        item => item,
    }
}

fn sanitize_message_content_part(
    part: openai::ResponseMessageOutputContentPart,
) -> openai::ResponseMessageOutputContentPart {
    match part {
        openai::ResponseMessageOutputContentPart::OutputText {
            annotations,
            logprobs,
            text,
            ..
        } => openai::ResponseMessageOutputContentPart::OutputText {
            annotations: annotations.into_iter().map(sanitize_annotation).collect(),
            logprobs,
            text,
            extra: Default::default(),
        },
        openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
            openai::ResponseMessageOutputContentPart::Refusal {
                refusal,
                extra: Default::default(),
            }
        }
    }
}

pub(super) fn stream_logprob(value: openai::StreamTokenLogprob) -> openai::TokenLogprob {
    openai::TokenLogprob {
        token: value.token,
        bytes: None,
        logprob: value.logprob,
        top_logprobs: value
            .top_logprobs
            .unwrap_or_default()
            .into_iter()
            .filter_map(|top| {
                Some(openai::TokenLogprobTop {
                    token: top.token?,
                    bytes: None,
                    logprob: top.logprob?,
                    extra: Default::default(),
                })
            })
            .collect(),
        extra: Default::default(),
    }
}

pub(super) fn sanitize_annotation(value: openai::ResponseAnnotation) -> openai::ResponseAnnotation {
    match value {
        openai::ResponseAnnotation::FileCitation {
            file_id,
            filename,
            index,
            ..
        } => openai::ResponseAnnotation::FileCitation {
            file_id,
            filename,
            index,
            extra: Default::default(),
        },
        openai::ResponseAnnotation::UrlCitation {
            end_index,
            start_index,
            title,
            url,
            ..
        } => openai::ResponseAnnotation::UrlCitation {
            end_index,
            start_index,
            title,
            url,
            extra: Default::default(),
        },
        openai::ResponseAnnotation::ContainerFileCitation {
            container_id,
            end_index,
            file_id,
            filename,
            start_index,
            ..
        } => openai::ResponseAnnotation::ContainerFileCitation {
            container_id,
            end_index,
            file_id,
            filename,
            start_index,
            extra: Default::default(),
        },
        openai::ResponseAnnotation::FilePath { file_id, index, .. } => {
            openai::ResponseAnnotation::FilePath {
                file_id,
                index,
                extra: Default::default(),
            }
        }
    }
}
