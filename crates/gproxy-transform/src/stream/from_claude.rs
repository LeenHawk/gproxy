use std::collections::BTreeMap;

use bytes::Bytes;
use serde_json::{Value, json};

use super::SseFrame;
use crate::TransformError;

#[derive(Clone, Copy)]
pub(super) enum Output {
    Chat,
    Responses,
}

pub(super) struct Converter {
    output: Output,
    id: String,
    model: String,
    blocks: BTreeMap<u64, Block>,
    completed: Vec<Value>,
    usage: Value,
    stop_reason: String,
    started: bool,
    stopped: bool,
}

enum Block {
    Text {
        id: String,
        text: String,
    },
    Thinking {
        id: String,
        text: String,
    },
    Tool {
        id: String,
        name: String,
        arguments: String,
    },
}

impl Converter {
    pub(super) fn new(output: Output) -> Self {
        Self {
            output,
            id: "resp_gproxy".into(),
            model: "unknown".into(),
            blocks: BTreeMap::new(),
            completed: Vec::new(),
            usage: json!({}),
            stop_reason: "end_turn".into(),
            started: false,
            stopped: false,
        }
    }

    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        let value: Value = serde_json::from_str(&frame.data)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .or(frame.event.as_deref())
            .ok_or_else(|| TransformError::shape("Claude SSE", "event type is missing"))?;
        match kind {
            "message_start" => self.message_start(&value),
            "content_block_start" => self.block_start(&value),
            "content_block_delta" => self.block_delta(&value),
            "content_block_stop" => self.block_stop(&value),
            "message_delta" => self.message_delta(&value),
            "message_stop" => self.message_stop(),
            "ping" => Ok(Vec::new()),
            "error" => Err(TransformError::unsupported("Claude SSE", "error")),
            other => Err(TransformError::unsupported("Claude SSE event", other)),
        }
    }

    pub(super) fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped || !self.started {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }

    fn message_start(&mut self, value: &Value) -> Result<Vec<Bytes>, TransformError> {
        let message = value.get("message").ok_or_else(|| {
            TransformError::shape("Claude SSE", "message_start.message is missing")
        })?;
        self.id = message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("resp_gproxy")
            .to_owned();
        self.model = message
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        self.usage = message.get("usage").cloned().unwrap_or_else(|| json!({}));
        self.started = true;
        Ok(match self.output {
            Output::Chat => {
                vec![self.chat_chunk(json!({"role":"assistant","content":""}), None, None)]
            }
            Output::Responses => {
                let response = self.response_object("in_progress");
                vec![SseFrame::json(
                    Some("response.created"),
                    json!({"type":"response.created","sequence_number":0,"response":response}),
                )]
            }
        })
    }

    fn block_start(&mut self, value: &Value) -> Result<Vec<Bytes>, TransformError> {
        let index = index(value)?;
        let block = value
            .get("content_block")
            .ok_or_else(|| TransformError::shape("Claude SSE", "content_block is missing"))?;
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| TransformError::shape("Claude SSE", "content block type is missing"))?;
        let id = block
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("item_{index}"));
        let (state, output) = match kind {
            "text" => {
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let output = match self.output {
                    Output::Chat => (!text.is_empty())
                        .then(|| self.chat_chunk(json!({"content":text}), None, None))
                        .into_iter()
                        .collect(),
                    Output::Responses => vec![
                        SseFrame::json(
                            Some("response.output_item.added"),
                            json!({
                                "type":"response.output_item.added","output_index":index,
                                "item":{"type":"message","id":id,"role":"assistant","content":[],"status":"in_progress"}
                            }),
                        ),
                        SseFrame::json(
                            Some("response.content_part.added"),
                            json!({
                                "type":"response.content_part.added","item_id":id,"output_index":index,
                                "content_index":0,"part":{"type":"output_text","text":"","annotations":[]}
                            }),
                        ),
                    ],
                };
                (Block::Text { id, text }, output)
            }
            "thinking" | "redacted_thinking" => {
                let text = block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let output = match self.output {
                    Output::Chat => (!text.is_empty())
                        .then(|| self.chat_chunk(json!({"reasoning_content":text}), None, None))
                        .into_iter()
                        .collect(),
                    Output::Responses => vec![SseFrame::json(
                        Some("response.output_item.added"),
                        json!({
                            "type":"response.output_item.added","output_index":index,
                            "item":{"type":"reasoning","id":id,"content":[],"summary":[],"status":"in_progress"}
                        }),
                    )],
                };
                (Block::Thinking { id, text }, output)
            }
            "tool_use" => {
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let output = match self.output {
                    Output::Chat => vec![self.chat_chunk(
                        json!({"tool_calls":[{
                            "index":index,"id":id,"type":"function",
                            "function":{"name":name,"arguments":""}
                        }]}),
                        None,
                        None,
                    )],
                    Output::Responses => vec![SseFrame::json(
                        Some("response.output_item.added"),
                        json!({
                            "type":"response.output_item.added","output_index":index,
                            "item":{"type":"function_call","id":id,"call_id":id,"name":name,"arguments":"","status":"in_progress"}
                        }),
                    )],
                };
                (
                    Block::Tool {
                        id,
                        name,
                        arguments: String::new(),
                    },
                    output,
                )
            }
            other => {
                return Err(TransformError::unsupported(
                    "Claude SSE content block",
                    other,
                ));
            }
        };
        self.blocks.insert(index, state);
        Ok(output)
    }

    fn block_delta(&mut self, value: &Value) -> Result<Vec<Bytes>, TransformError> {
        let index = index(value)?;
        let delta = value
            .get("delta")
            .ok_or_else(|| TransformError::shape("Claude SSE", "delta is missing"))?;
        let kind = delta
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let text = match kind {
            "text_delta" => delta.get("text"),
            "thinking_delta" | "signature_delta" => {
                delta.get("thinking").or_else(|| delta.get("signature"))
            }
            "input_json_delta" => delta.get("partial_json"),
            other => return Err(TransformError::unsupported("Claude SSE delta", other)),
        }
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
        let block = self
            .blocks
            .get_mut(&index)
            .ok_or_else(|| TransformError::shape("Claude SSE", "delta precedes block start"))?;
        let output = match (self.output, block) {
            (Output::Chat, Block::Text { text: total, .. }) => {
                total.push_str(&text);
                vec![self.chat_chunk(json!({"content":text}), None, None)]
            }
            (Output::Chat, Block::Thinking { text: total, .. }) => {
                total.push_str(&text);
                vec![self.chat_chunk(json!({"reasoning_content":text}), None, None)]
            }
            (Output::Chat, Block::Tool { arguments, .. }) => {
                arguments.push_str(&text);
                vec![self.chat_chunk(
                    json!({"tool_calls":[{"index":index,"function":{"arguments":text}}]}),
                    None,
                    None,
                )]
            }
            (Output::Responses, Block::Text { id, text: total }) => {
                total.push_str(&text);
                vec![SseFrame::json(
                    Some("response.output_text.delta"),
                    json!({
                        "type":"response.output_text.delta","item_id":id,"output_index":index,"content_index":0,"delta":text
                    }),
                )]
            }
            (Output::Responses, Block::Thinking { id, text: total }) => {
                total.push_str(&text);
                vec![SseFrame::json(
                    Some("response.reasoning_text.delta"),
                    json!({
                        "type":"response.reasoning_text.delta","item_id":id,"output_index":index,"content_index":0,"delta":text
                    }),
                )]
            }
            (Output::Responses, Block::Tool { id, arguments, .. }) => {
                arguments.push_str(&text);
                vec![SseFrame::json(
                    Some("response.function_call_arguments.delta"),
                    json!({
                        "type":"response.function_call_arguments.delta","item_id":id,"output_index":index,"delta":text
                    }),
                )]
            }
        };
        Ok(output)
    }

    fn block_stop(&mut self, value: &Value) -> Result<Vec<Bytes>, TransformError> {
        let index = index(value)?;
        let block = self
            .blocks
            .remove(&index)
            .ok_or_else(|| TransformError::shape("Claude SSE", "block stop precedes start"))?;
        if matches!(self.output, Output::Chat) {
            return Ok(Vec::new());
        }
        let (item, mut output) = match block {
            Block::Text { id, text } => {
                let item = json!({
                    "type":"message","id":id,"role":"assistant",
                    "content":[{"type":"output_text","text":text,"annotations":[]}],"status":"completed"
                });
                let output = vec![
                    SseFrame::json(
                        Some("response.output_text.done"),
                        json!({
                            "type":"response.output_text.done","item_id":id,"output_index":index,"content_index":0,"text":text
                        }),
                    ),
                    SseFrame::json(
                        Some("response.content_part.done"),
                        json!({
                            "type":"response.content_part.done","item_id":id,"output_index":index,"content_index":0,
                            "part":{"type":"output_text","text":text,"annotations":[]}
                        }),
                    ),
                ];
                (item, output)
            }
            Block::Thinking { id, text } => (
                json!({"type":"reasoning","id":id,"content":[{"type":"reasoning_text","text":text}],"summary":[],"status":"completed"}),
                Vec::new(),
            ),
            Block::Tool {
                id,
                name,
                arguments,
            } => (
                json!({"type":"function_call","id":id,"call_id":id,"name":name,"arguments":arguments,"status":"completed"}),
                vec![SseFrame::json(
                    Some("response.function_call_arguments.done"),
                    json!({
                        "type":"response.function_call_arguments.done","item_id":id,"output_index":index,"arguments":arguments
                    }),
                )],
            ),
        };
        output.push(SseFrame::json(
            Some("response.output_item.done"),
            json!({
                "type":"response.output_item.done","output_index":index,"item":item
            }),
        ));
        self.completed.push(item);
        Ok(output)
    }

    fn message_delta(&mut self, value: &Value) -> Result<Vec<Bytes>, TransformError> {
        if let Some(reason) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
            self.stop_reason = reason.into();
        }
        if let Some(usage) = value.get("usage") {
            merge(&mut self.usage, usage);
        }
        Ok(match self.output {
            Output::Chat => vec![self.chat_chunk(
                json!({}),
                Some(crate::content::common::stop_to_openai(Some(
                    &self.stop_reason,
                ))),
                Some(crate::content::common::usage_to_openai(
                    Some(&self.usage),
                    true,
                )),
            )],
            Output::Responses => Vec::new(),
        })
    }

    fn message_stop(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if !self.blocks.is_empty() {
            return Err(TransformError::shape(
                "Claude SSE",
                "message stopped with open blocks",
            ));
        }
        self.stopped = true;
        Ok(match self.output {
            Output::Chat => vec![SseFrame::encode(None, "[DONE]")],
            Output::Responses => {
                let response = self.response_object("completed");
                vec![SseFrame::json(
                    Some("response.completed"),
                    json!({
                        "type":"response.completed","response":response
                    }),
                )]
            }
        })
    }

    fn chat_chunk(&self, delta: Value, finish_reason: Option<&str>, usage: Option<Value>) -> Bytes {
        SseFrame::json(
            None,
            json!({
                "id":self.id,"object":"chat.completion.chunk","created":0,"model":self.model,
                "choices":[{"index":0,"delta":delta,"finish_reason":finish_reason}],"usage":usage
            }),
        )
    }

    fn response_object(&self, status: &str) -> Value {
        json!({
            "id":self.id,"object":"response","created_at":0,"status":status,
            "model":self.model,"output":self.completed,
            "output_text":self.completed.iter().filter_map(|item| item.pointer("/content/0/text").and_then(Value::as_str)).collect::<String>(),
            "usage":crate::content::common::usage_to_openai(Some(&self.usage), false)
        })
    }
}

fn index(value: &Value) -> Result<u64, TransformError> {
    value
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| TransformError::shape("Claude SSE", "content block index is missing"))
}

fn merge(target: &mut Value, update: &Value) {
    let Some(target) = target.as_object_mut() else {
        *target = update.clone();
        return;
    };
    if let Some(update) = update.as_object() {
        target.extend(update.clone());
    }
}
