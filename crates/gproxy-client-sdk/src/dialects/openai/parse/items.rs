use super::*;
pub(crate) fn parse_item(item: r::ResponseOutputItem, index: u32) -> Result<Vec<Block>> {
    Ok(match item {
        r::ResponseOutputItem::Message(message) => parse_message(message, index)?,

        r::ResponseOutputItem::FunctionCall(call) => parse_function_call(call)?,

        r::ResponseOutputItem::Reasoning(item) => parse_reasoning(item)?,

        r::ResponseOutputItem::FunctionCallOutput(value) => vec![parse_tool_result(value)?],
        r::ResponseOutputItem::ImageGenerationCall(value) => parse_image(value)?,

        r::ResponseOutputItem::WebSearchCall(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "web_search".into(),
                arguments: serde_json::to_string(&value.action).map_err(Error::invalid)?,
                provider_executed: true,
            });
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Web {
                    status: value.status,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?]
        }
        r::ResponseOutputItem::FileSearchCall(mut value) => {
            let outputs = value.results.take();
            let id = value.id.clone();
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "file_search".into(),
                arguments: serde_json::to_string(&value.queries).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut blocks = vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Files {
                    status: value.status,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?];
            if let Some(outputs) = outputs {
                blocks.push(server_output(
                    id,
                    "file_search",
                    serde_json::to_value(outputs).map_err(Error::invalid)?,
                    ServerOutputKind::FileSearch,
                )?);
            }
            blocks
        }
        r::ResponseOutputItem::CodeInterpreterCall(mut value) => {
            let outputs = value.outputs.take();
            let id = value.id.clone();
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "code_interpreter".into(),
                arguments: serde_json::to_string(&value.code).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut blocks = vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Code {
                    status: value.status,
                    container_id: value.container_id,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?];
            if let Some(outputs) = outputs {
                blocks.push(server_output(
                    id,
                    "code_interpreter",
                    serde_json::to_value(outputs).map_err(Error::invalid)?,
                    ServerOutputKind::CodeInterpreter,
                )?);
            }
            blocks
        }
        r::ResponseOutputItem::ComputerCall(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.call_id),
                name: "computer".into(),
                arguments: serde_json::to_string(&ComputerArguments {
                    action: value.action,
                    actions: value.actions,
                })
                .map_err(Error::invalid)?,
                provider_executed: true,
            });
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Computer {
                    id: value.id,
                    status: value.status,
                    pending_safety_checks: value.pending_safety_checks,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?]
        }
        other => vec![with_native(Content::Opaque, Vendor::Openai, other)?],
    })
}

fn parse_message(mut message: i::ResponseOutputMessage, index: u32) -> Result<Vec<Block>> {
    Ok({
        let parts = std::mem::take(&mut message.content);
        let envelope = MessageMetadata {
            id: message.id,
            status: message.status,
            agent: message.agent,
            phase: message.phase,
            rest: message.rest,
        };
        parts
            .into_iter()
            .map(|part| {
                let (content, native) = match part {
                    i::OutputContent::Text(value) => {
                        let citations = value
                            .annotations
                            .into_iter()
                            .map(citations::parse_responses)
                            .collect::<Result<_>>()?;
                        (
                            Content::Text(Text {
                                refusal: false,
                                text: value.text,
                                citations,
                            }),
                            TextMetadata {
                                logprobs: value.logprobs,
                                rest: value.rest,
                            },
                        )
                    }
                    i::OutputContent::Refusal(value) => (
                        Content::Text(Text {
                            refusal: true,
                            text: value.refusal,
                            citations: Vec::new(),
                        }),
                        TextMetadata {
                            logprobs: Vec::new(),
                            rest: value.rest,
                        },
                    ),
                };
                with_metadata(
                    content,
                    Vendor::Openai,
                    PartMetadata {
                        native: Some(native),
                        item: Some(envelope.clone()),
                        group: Some(index),
                    },
                )
            })
            .collect::<Result<_>>()?
    })
}
fn parse_function_call(mut call: i::FunctionCall) -> Result<Vec<Block>> {
    Ok({
        let content = Content::ToolCall(ToolCall {
            id: std::mem::take(&mut call.call_id),
            name: std::mem::take(&mut call.name),
            arguments: std::mem::take(&mut call.arguments),
            provider_executed: false,
        });
        let meta = FunctionMetadata {
            id: call.id,
            namespace: call.namespace,
            status: call.status,
            async_: call.async_,
            agent: call.agent,
            caller: call.caller,
            rest: call.rest,
        };
        vec![with_native(content, Vendor::Openai, meta)?]
    })
}
fn parse_reasoning(item: i::ReasoningItem) -> Result<Vec<Block>> {
    Ok({
        let text = item.summary.iter().map(|v| v.text.as_str()).collect();
        let summary = item
            .summary
            .into_iter()
            .map(|v| SummaryMetadata {
                chars: v.text.chars().count(),
                rest: v.rest,
            })
            .collect();
        let meta = ReasoningMetadata {
            id: item.id,
            summary,
            content: item.content,
            status: item.status,
            agent: item.agent,
            encrypted_content: item.encrypted_content,
            rest: item.rest,
        };
        vec![with_native(
            Content::Reasoning(Reasoning { text }),
            Vendor::Openai,
            meta,
        )?]
    })
}
fn parse_image(mut value: i::ImageGenerationCall) -> Result<Vec<Block>> {
    Ok({
        if let Some(data) = value.result.take() {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(Error::invalid)?;
            let mime = value
                .output_format
                .as_ref()
                .and_then(Option::as_ref)
                .map(|f| {
                    match f {
                        t::ImageOutputFormat::Png => "image/png",
                        t::ImageOutputFormat::Jpeg => "image/jpeg",
                        t::ImageOutputFormat::Webp => "image/webp",
                    }
                    .to_owned()
                });
            let content = Content::Image(Image {
                source: Asset::Inline { bytes },
                media_type: mime,
                detail: Detail::Auto,
            });
            vec![with_native(
                content,
                Vendor::Openai,
                ImageMetadata::Generated(GeneratedImageMetadata {
                    id: value.id,
                    status: value.status,
                    error: value.error,
                    agent: value.agent,
                    action: value.action,
                    background: value.background,
                    format_present: value.output_format.is_some(),
                    quality: value.quality,
                    revised_prompt: value.revised_prompt,
                    size: value.size,
                    rest: value.rest,
                }),
            )?]
        } else {
            vec![with_native(
                Content::Opaque,
                Vendor::Openai,
                r::ResponseOutputItem::ImageGenerationCall(value),
            )?]
        }
    })
}
