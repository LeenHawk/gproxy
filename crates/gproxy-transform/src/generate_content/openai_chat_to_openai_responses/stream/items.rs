use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::native::source_id;
use super::wire::{empty_delta, response_item_name};
use super::{State, Tool, ToolKind, ToolStart};

impl State {
    pub(super) fn complete_item(
        &mut self,
        item: openai::ResponseItem,
        output_index: u32,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        match item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                let mut output = Vec::new();
                let mut rest = message.rest;
                rest.insert("status".into(), serde_json::to_value(message.status)?);
                if let Some(phase) = message.phase {
                    rest.insert("phase".into(), serde_json::to_value(phase)?);
                }
                for part in message.content {
                    output.extend(self.complete_message_part(part, event_rest.clone())?);
                }
                if !rest.is_empty() {
                    output.push(self.preserve(rest, Default::default())?);
                } else if output.is_empty() && !event_rest.is_empty() {
                    output.push(self.preserve(Default::default(), event_rest)?);
                }
                Ok(output)
            }
            openai::ResponseItem::Typed(item) => match *item {
                openai::TypedResponseItem::FunctionCall {
                    arguments,
                    call_id,
                    name,
                    id,
                    caller,
                    namespace,
                    status,
                    mut rest,
                } => {
                    let source_id = source_id(id.as_deref(), output_index);
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "namespace", namespace)?;
                    preserve_option(&mut rest, "status", status)?;
                    let mut output = self.start_tool(ToolStart {
                        source_id: source_id.clone(),
                        call_id,
                        output_index,
                        name,
                        kind: ToolKind::Function,
                        rest,
                        event_rest,
                    })?;
                    output.extend(self.finish_tool(
                        &source_id,
                        output_index,
                        ToolKind::Function,
                        arguments,
                        Default::default(),
                    )?);
                    Ok(output)
                }
                openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    input,
                    name,
                    id,
                    caller,
                    namespace,
                    mut rest,
                } => {
                    let source_id = source_id(id.as_deref(), output_index);
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "namespace", namespace)?;
                    let mut output = self.start_tool(ToolStart {
                        source_id: source_id.clone(),
                        call_id,
                        output_index,
                        name,
                        kind: ToolKind::Custom,
                        rest,
                        event_rest,
                    })?;
                    output.extend(self.finish_tool(
                        &source_id,
                        output_index,
                        ToolKind::Custom,
                        input,
                        Default::default(),
                    )?);
                    Ok(output)
                }
                openai::TypedResponseItem::ShellCall {
                    action,
                    call_id,
                    id,
                    caller,
                    environment,
                    status,
                    created_by,
                    mut rest,
                } => {
                    let source_id = source_id(id.as_deref(), output_index);
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "environment", environment)?;
                    preserve_option(&mut rest, "status", status)?;
                    preserve_option(&mut rest, "created_by", created_by)?;
                    self.complete_function_item(
                        ToolStart {
                            source_id,
                            call_id,
                            output_index,
                            name: "shell".into(),
                            kind: ToolKind::Function,
                            rest,
                            event_rest,
                        },
                        serde_json::to_string(&action)?,
                    )
                }
                openai::TypedResponseItem::ApplyPatchCall {
                    call_id,
                    operation,
                    status,
                    id,
                    caller,
                    created_by,
                    mut rest,
                } => {
                    let source_id = source_id(id.as_deref(), output_index);
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "status", Some(status))?;
                    preserve_option(&mut rest, "created_by", created_by)?;
                    self.complete_function_item(
                        ToolStart {
                            source_id,
                            call_id,
                            output_index,
                            name: "apply_patch".into(),
                            kind: ToolKind::Function,
                            rest,
                            event_rest,
                        },
                        serde_json::to_string(&operation)?,
                    )
                }
                openai::TypedResponseItem::Reasoning {
                    summary,
                    content,
                    encrypted_content,
                    status,
                    mut rest,
                    ..
                } => {
                    if encrypted_content.is_some() {
                        return Err(TransformError::unsupported(
                            "Responses stream",
                            "encrypted reasoning content",
                        ));
                    }
                    preserve_option(&mut rest, "status", status)?;
                    let mut output = Vec::new();
                    for part in summary {
                        output.extend(self.finish_reasoning(
                            part.text,
                            part.rest,
                            event_rest.clone(),
                        )?);
                    }
                    for part in content.into_iter().flatten() {
                        output.extend(self.finish_reasoning(
                            part.text,
                            part.rest,
                            event_rest.clone(),
                        )?);
                    }
                    if !rest.is_empty() {
                        output.push(self.preserve(rest, Default::default())?);
                    } else if output.is_empty() && !event_rest.is_empty() {
                        output.push(self.preserve(Default::default(), event_rest)?);
                    }
                    Ok(output)
                }
                other => Err(TransformError::unsupported(
                    "Responses output item",
                    response_item_name(&other),
                )),
            },
            openai::ResponseItem::Message(openai::ResponseMessageItem::Input(_)) => Err(
                TransformError::unsupported("Responses output item", "input message"),
            ),
            openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(_)) => Err(
                TransformError::unsupported("Responses output item", "easy input message"),
            ),
            openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(value)) => Err(
                TransformError::unsupported("Responses output item", value.to_string()),
            ),
            openai::ResponseItem::Unknown(value) => Err(TransformError::unsupported(
                "Responses output item",
                value.to_string(),
            )),
        }
    }

    fn complete_message_part(
        &mut self,
        part: openai::ResponseMessageOutputContentPart,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        match part {
            openai::ResponseMessageOutputContentPart::OutputText(part) => {
                if !part.annotations.is_empty() || part.logprobs.is_some() {
                    return Err(TransformError::unsupported(
                        "Responses output text",
                        "annotations or logprobs",
                    ));
                }
                self.finish_text(part.text, part.rest, event_rest)
            }
            openai::ResponseMessageOutputContentPart::Refusal(part) => {
                self.finish_refusal(part.refusal, part.rest, event_rest)
            }
            openai::ResponseMessageOutputContentPart::Unknown(value) => Err(
                TransformError::unsupported("Responses content part", value.to_string()),
            ),
        }
    }

    pub(super) fn complete_part(
        &mut self,
        part: openai::ResponseContentPart,
        _item_id: String,
        _output_index: u32,
        _content_index: u32,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        match part {
            openai::ResponseContentPart::OutputText(part) => {
                if !part.annotations.is_empty() || part.logprobs.is_some() {
                    return Err(TransformError::unsupported(
                        "Responses output text",
                        "annotations or logprobs",
                    ));
                }
                self.finish_text(part.text, part.rest, event_rest)
            }
            openai::ResponseContentPart::Refusal(part) => {
                self.finish_refusal(part.refusal, part.rest, event_rest)
            }
            openai::ResponseContentPart::ReasoningText(part) => {
                self.finish_reasoning(part.text, part.rest, event_rest)
            }
            openai::ResponseContentPart::Unknown(value) => Err(TransformError::unsupported(
                "Responses content part",
                value.to_string(),
            )),
        }
    }

    pub(super) fn finish_text(
        &mut self,
        full: String,
        rest: openai::Rest,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let delta = suffix(&self.text, &full, "output text")?;
        self.text = full;
        self.content_chunk(delta, rest, event_rest, ContentKind::Text)
    }

    pub(super) fn finish_reasoning(
        &mut self,
        full: String,
        rest: openai::Rest,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let delta = suffix(&self.reasoning, &full, "reasoning text")?;
        self.reasoning = full;
        self.content_chunk(delta, rest, event_rest, ContentKind::Reasoning)
    }

    pub(super) fn finish_refusal(
        &mut self,
        full: String,
        rest: openai::Rest,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let delta = suffix(&self.refusal, &full, "refusal")?;
        self.refusal = full;
        self.content_chunk(delta, rest, event_rest, ContentKind::Refusal)
    }

    fn content_chunk(
        &self,
        delta: String,
        rest: openai::Rest,
        event_rest: openai::Rest,
        kind: ContentKind,
    ) -> Result<Vec<Bytes>, TransformError> {
        if delta.is_empty() && rest.is_empty() && event_rest.is_empty() {
            return Ok(Vec::new());
        }
        let mut value = empty_delta();
        match kind {
            ContentKind::Text => value.content = Some(delta),
            ContentKind::Reasoning => value.reasoning_content = Some(delta),
            ContentKind::Refusal => value.refusal = Some(delta),
        }
        value.rest = rest;
        Ok(vec![self.chunk(value, None, None, event_rest)?])
    }

    pub(super) fn start_tool(&mut self, start: ToolStart) -> Result<Vec<Bytes>, TransformError> {
        let ToolStart {
            source_id,
            call_id,
            output_index,
            name,
            kind,
            rest,
            event_rest,
        } = start;
        if let Some(tool) = self.tools.get(&source_id) {
            if tool.kind != kind
                || tool.output_index != output_index
                || tool.call_id != call_id
                || tool.name != name
            {
                return Err(TransformError::shape(
                    "Responses stream",
                    "tool output item changed kind or index",
                ));
            }
            return if rest.is_empty() && event_rest.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![self.preserve(rest, event_rest)?])
            };
        }
        let index = self.next_tool;
        self.next_tool += 1;
        self.tools.insert(
            source_id,
            Tool {
                index,
                output_index,
                kind,
                call_id: call_id.clone(),
                name: name.clone(),
                data: String::new(),
            },
        );
        let call = openai::ChatToolCallDelta {
            index,
            id: Some(call_id),
            type_: Some(match kind {
                ToolKind::Function => openai::ChatToolCallType::Function,
                ToolKind::Custom => openai::ChatToolCallType::Custom,
            }),
            function: (kind == ToolKind::Function).then(|| openai::FunctionCallDelta {
                arguments: None,
                name: Some(name.clone()),
                rest: Default::default(),
            }),
            custom: (kind == ToolKind::Custom).then(|| openai::CustomToolCallDelta {
                input: None,
                name: Some(name),
                rest: Default::default(),
            }),
            rest,
        };
        Ok(vec![self.chunk(
            openai::ChatDelta {
                tool_calls: Some(vec![call]),
                ..empty_delta()
            },
            None,
            None,
            event_rest,
        )?])
    }

    pub(super) fn finish_tool(
        &mut self,
        id: &str,
        output_index: u32,
        kind: ToolKind,
        full: String,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let tool = self.tools.get_mut(id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool done before output item")
        })?;
        if tool.kind != kind || tool.output_index != output_index {
            return Err(TransformError::shape(
                "Responses stream",
                "tool done does not match its output item",
            ));
        }
        let delta = suffix(&tool.data, &full, "tool input")?;
        tool.data = full;
        let index = tool.index;
        if delta.is_empty() && event_rest.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![self.tool_chunk(index, kind, delta, event_rest)?])
        }
    }

    pub(super) fn tool_chunk(
        &self,
        index: u32,
        kind: ToolKind,
        delta: String,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        self.chunk(
            openai::ChatDelta {
                tool_calls: Some(vec![openai::ChatToolCallDelta {
                    index,
                    id: None,
                    type_: None,
                    function: (kind == ToolKind::Function).then(|| openai::FunctionCallDelta {
                        arguments: Some(delta.clone()),
                        name: None,
                        rest: Default::default(),
                    }),
                    custom: (kind == ToolKind::Custom).then(|| openai::CustomToolCallDelta {
                        input: Some(delta),
                        name: None,
                        rest: Default::default(),
                    }),
                    rest: Default::default(),
                }]),
                ..empty_delta()
            },
            None,
            None,
            rest,
        )
    }

    fn preserve(
        &self,
        delta_rest: openai::Rest,
        event_rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        let mut delta = empty_delta();
        delta.rest = delta_rest;
        self.chunk(delta, None, None, event_rest)
    }
}

enum ContentKind {
    Text,
    Reasoning,
    Refusal,
}

fn suffix(current: &str, full: &str, name: &str) -> Result<String, TransformError> {
    full.strip_prefix(current)
        .map(str::to_owned)
        .ok_or_else(|| {
            TransformError::shape(
                "Responses stream",
                format!("{name} done value does not extend prior deltas"),
            )
        })
}

fn preserve_option<T: serde::Serialize>(
    rest: &mut openai::Rest,
    key: &str,
    value: Option<T>,
) -> Result<(), TransformError> {
    if let Some(value) = value {
        rest.insert(key.into(), serde_json::to_value(value)?);
    }
    Ok(())
}
