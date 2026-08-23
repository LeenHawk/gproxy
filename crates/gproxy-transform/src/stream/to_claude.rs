use std::collections::BTreeMap;

use bytes::Bytes;
use serde_json::{Value, json};

use super::SseFrame;
use crate::TransformError;

#[derive(Clone, Copy)]
pub(super) enum Input {
    Chat,
    Responses,
}

pub(super) struct Converter {
    input: Input,
    id: String,
    model: String,
    started: bool,
    stopped: bool,
    next_index: u64,
    open: Option<OpenBlock>,
    tools: BTreeMap<String, Tool>,
    usage: Value,
    stop_reason: String,
    delta_sent: bool,
}

struct OpenBlock {
    key: String,
    index: u64,
    kind: BlockKind,
}

#[derive(Clone, Copy)]
enum BlockKind {
    Text,
    Thinking,
}

struct Tool {
    id: String,
    name: String,
    arguments: String,
}

impl Converter {
    pub(super) fn new(input: Input) -> Self {
        Self {
            input,
            id: "msg_gproxy".into(),
            model: "unknown".into(),
            started: false,
            stopped: false,
            next_index: 0,
            open: None,
            tools: BTreeMap::new(),
            usage: json!({}),
            stop_reason: "end_turn".into(),
            delta_sent: false,
        }
    }

    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        if matches!(self.input, Input::Chat) {
            self.chat(frame)
        } else {
            self.responses(frame)
        }
    }

    pub(super) fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped || !self.started {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }

    fn chat(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        if frame.data == "[DONE]" {
            return self.complete();
        }
        let value: Value = serde_json::from_str(&frame.data)?;
        if value.get("error").is_some() {
            return Err(TransformError::unsupported("OpenAI Chat SSE", "error"));
        }
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            self.id = id.into();
        }
        if let Some(model) = value.get("model").and_then(Value::as_str) {
            self.model = model.into();
        }
        if let Some(usage) = value.get("usage").filter(|usage| usage.is_object()) {
            self.usage = crate::content::common::usage_to_claude(Some(usage), true);
        }
        let mut output = self.ensure_start();
        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return Ok(output);
        };
        let delta = choice.get("delta").cloned().unwrap_or_else(|| json!({}));
        if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str) {
            output.extend(self.scalar_delta("thinking", BlockKind::Thinking, reasoning)?);
        }
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            output.extend(self.scalar_delta("text", BlockKind::Text, text)?);
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                let index = call
                    .get("index")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let key = format!("tool:{index}");
                let tool = self.tools.entry(key).or_insert_with(|| Tool {
                    id: call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("tool_gproxy")
                        .into(),
                    name: call
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    arguments: String::new(),
                });
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    tool.id = id.into();
                }
                if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                    tool.name = name.into();
                }
                if let Some(arguments) = call.pointer("/function/arguments").and_then(Value::as_str)
                {
                    tool.arguments.push_str(arguments);
                }
            }
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.stop_reason = crate::content::common::stop_to_claude(Some(reason)).into();
            output.extend(self.finish_message(false)?);
        }
        Ok(output)
    }

    fn responses(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        let value: Value = serde_json::from_str(&frame.data)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .or(frame.event.as_deref())
            .ok_or_else(|| {
                TransformError::shape("OpenAI Responses SSE", "event type is missing")
            })?;
        if let Some(response) = value.get("response") {
            if let Some(id) = response.get("id").and_then(Value::as_str) {
                self.id = id.into();
            }
            if let Some(model) = response.get("model").and_then(Value::as_str) {
                self.model = model.into();
            }
            if let Some(usage) = response.get("usage").filter(|usage| usage.is_object()) {
                self.usage = crate::content::common::usage_to_claude(Some(usage), false);
            }
        }
        let mut output = self.ensure_start();
        match kind {
            "response.created" | "response.in_progress" | "response.queued" => {}
            "response.output_item.added" => {
                let item = value.get("item").ok_or_else(|| {
                    TransformError::shape("OpenAI Responses SSE", "item is missing")
                })?;
                match item.get("type").and_then(Value::as_str) {
                    Some("function_call" | "custom_tool_call") => {
                        let key = item
                            .get("id")
                            .or_else(|| item.get("call_id"))
                            .and_then(Value::as_str)
                            .unwrap_or("tool_gproxy")
                            .to_owned();
                        self.tools.insert(
                            key,
                            Tool {
                                id: item
                                    .get("id")
                                    .or_else(|| item.get("call_id"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("tool_gproxy")
                                    .into(),
                                name: item
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .into(),
                                arguments: item
                                    .get("arguments")
                                    .or_else(|| item.get("input"))
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .into(),
                            },
                        );
                    }
                    Some("reasoning") => {}
                    Some("message") => {}
                    Some(other) => {
                        return Err(TransformError::unsupported(
                            "OpenAI Responses output item",
                            other,
                        ));
                    }
                    None => {
                        return Err(TransformError::shape(
                            "OpenAI Responses SSE",
                            "item type is missing",
                        ));
                    }
                }
            }
            "response.content_part.added" => {
                if value.pointer("/part/type").and_then(Value::as_str) == Some("output_text") {
                    let key = value
                        .get("item_id")
                        .and_then(Value::as_str)
                        .unwrap_or("text");
                    output.extend(self.open_scalar(key, BlockKind::Text)?);
                }
            }
            "response.output_text.delta" => {
                let key = value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or("text");
                let text = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                output.extend(self.scalar_delta(key, BlockKind::Text, text)?);
            }
            "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
                let key = value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or("thinking");
                let text = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                output.extend(self.scalar_delta(key, BlockKind::Thinking, text)?);
            }
            "response.function_call_arguments.delta" => {
                let key = value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or("tool_gproxy");
                self.tools
                    .entry(key.into())
                    .or_insert_with(|| Tool {
                        id: key.into(),
                        name: String::new(),
                        arguments: String::new(),
                    })
                    .arguments
                    .push_str(
                        value
                            .get("delta")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    );
            }
            "response.output_text.done"
            | "response.reasoning_text.done"
            | "response.reasoning_summary_text.done"
            | "response.content_part.done"
            | "response.output_item.done"
            | "response.function_call_arguments.done" => {}
            "response.completed" => {
                self.stop_reason = "end_turn".into();
                output.extend(self.finish_message(true)?);
            }
            "response.incomplete" => {
                self.stop_reason = "max_tokens".into();
                output.extend(self.finish_message(true)?);
            }
            "response.failed" | "error" => {
                return Err(TransformError::unsupported("OpenAI Responses SSE", kind));
            }
            other => {
                return Err(TransformError::unsupported(
                    "OpenAI Responses SSE event",
                    other,
                ));
            }
        }
        Ok(output)
    }

    fn ensure_start(&mut self) -> Vec<Bytes> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![SseFrame::json(
            Some("message_start"),
            json!({
                "type":"message_start",
                "message":{
                    "id":self.id,"type":"message","role":"assistant","model":self.model,
                    "content":[],"stop_reason":null,"stop_sequence":null,
                    "usage":{"input_tokens":0,"output_tokens":0}
                }
            }),
        )]
    }

    fn scalar_delta(
        &mut self,
        key: &str,
        kind: BlockKind,
        text: &str,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = self.open_scalar(key, kind)?;
        let index = self.open.as_ref().expect("opened").index;
        let delta = match kind {
            BlockKind::Text => json!({"type":"text_delta","text":text}),
            BlockKind::Thinking => {
                json!({"type":"thinking_delta","thinking":text})
            }
        };
        output.push(SseFrame::json(
            Some("content_block_delta"),
            json!({
                "type":"content_block_delta","index":index,
                "delta":delta
            }),
        ));
        Ok(output)
    }

    fn open_scalar(&mut self, key: &str, kind: BlockKind) -> Result<Vec<Bytes>, TransformError> {
        if self.open.as_ref().is_some_and(|open| open.key == key) {
            return Ok(Vec::new());
        }
        let mut output = self.close_scalar();
        let index = self.next_index;
        self.next_index += 1;
        let block = match kind {
            BlockKind::Text => json!({"type":"text","text":""}),
            BlockKind::Thinking => json!({"type":"thinking","thinking":"","signature":""}),
        };
        output.push(SseFrame::json(
            Some("content_block_start"),
            json!({
                "type":"content_block_start","index":index,"content_block":block
            }),
        ));
        self.open = Some(OpenBlock {
            key: key.into(),
            index,
            kind,
        });
        Ok(output)
    }

    fn close_scalar(&mut self) -> Vec<Bytes> {
        self.open
            .take()
            .map(|open| {
                let _ = open.kind;
                SseFrame::json(
                    Some("content_block_stop"),
                    json!({
                        "type":"content_block_stop","index":open.index
                    }),
                )
            })
            .into_iter()
            .collect()
    }

    fn flush_tools(&mut self) -> Vec<Bytes> {
        let mut output = Vec::new();
        for (_, tool) in std::mem::take(&mut self.tools) {
            let index = self.next_index;
            self.next_index += 1;
            output.push(SseFrame::json(
                Some("content_block_start"),
                json!({
                    "type":"content_block_start","index":index,
                    "content_block":{"type":"tool_use","id":tool.id,"name":tool.name,"input":{}}
                }),
            ));
            if !tool.arguments.is_empty() {
                output.push(SseFrame::json(
                    Some("content_block_delta"),
                    json!({
                        "type":"content_block_delta","index":index,
                        "delta":{"type":"input_json_delta","partial_json":tool.arguments}
                    }),
                ));
            }
            output.push(SseFrame::json(
                Some("content_block_stop"),
                json!({
                    "type":"content_block_stop","index":index
                }),
            ));
        }
        output
    }

    fn finish_message(&mut self, stop: bool) -> Result<Vec<Bytes>, TransformError> {
        let mut output = self.close_scalar();
        output.extend(self.flush_tools());
        if !self.delta_sent {
            output.push(SseFrame::json(
                Some("message_delta"),
                json!({
                    "type":"message_delta",
                    "delta":{"stop_reason":self.stop_reason,"stop_sequence":null},
                    "usage":self.usage
                }),
            ));
            self.delta_sent = true;
        }
        if stop {
            output.extend(self.complete()?);
        }
        Ok(output)
    }

    fn complete(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            return Ok(Vec::new());
        }
        let mut output = self.close_scalar();
        output.extend(self.flush_tools());
        if !self.delta_sent {
            output.push(SseFrame::json(
                Some("message_delta"),
                json!({
                    "type":"message_delta",
                    "delta":{"stop_reason":self.stop_reason,"stop_sequence":null},
                    "usage":self.usage
                }),
            ));
        }
        output.push(SseFrame::json(
            Some("message_stop"),
            json!({"type":"message_stop"}),
        ));
        self.stopped = true;
        Ok(output)
    }
}
