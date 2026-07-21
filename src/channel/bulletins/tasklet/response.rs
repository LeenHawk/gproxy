use std::collections::{BTreeMap, HashMap, HashSet};

use bytes::Bytes;
use serde_json::Value;

use crate::protocol::openai::{
    ChatChunkChoice, ChatCompletionChunk, ChatCompletionChunkObjectType, ChatDelta, ChatDeltaRole,
    ChatFinishReason, ChatToolCallDelta, ChatToolCallType, CustomToolCallDelta, FunctionCallDelta,
    OpenAiModelId,
};
use crate::transform::common::sse::SseFrame;

use super::bridge::InvocationPayload;

pub struct Absorbed {
    pub chunks: Vec<Bytes>,
    pub finished: bool,
}

pub struct Synth {
    id: String,
    model: String,
    created: u64,
    previous: HashMap<String, String>,
    tools: HashSet<String>,
    seen_running: bool,
    emit_tool_trace: bool,
}

impl Synth {
    pub fn new(model: String, emit_tool_trace: bool) -> Self {
        Self {
            id: format!("chatcmpl-{}", crate::util::rand::uuid_v4().replace('-', "")),
            model,
            created: crate::util::time::unix_now().max(0) as u64,
            previous: HashMap::new(),
            tools: HashSet::new(),
            seen_running: false,
            emit_tool_trace,
        }
    }

    pub fn initial(&self) -> Result<Bytes, String> {
        self.delta(ChatDelta {
            role: Some(ChatDeltaRole::Assistant),
            content: None,
            reasoning_content: None,
            refusal: None,
            tool_calls: None,
            function_call: None,
            obfuscation: None,
            extra: BTreeMap::new(),
        })
    }

    pub fn absorb(&mut self, text: &str) -> Result<Absorbed, String> {
        let frame: Value = serde_json::from_str(text)
            .map_err(|error| format!("tasklet websocket JSON: {error}"))?;
        match frame.get("type").and_then(Value::as_str) {
            Some("syncUpdate") => self.sync_update(&frame),
            Some("blocksUpdate") => self.blocks_update(&frame),
            Some("error") => Err(format!(
                "tasklet websocket error: {}",
                frame
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
            )),
            _ => Ok(Absorbed {
                chunks: Vec::new(),
                finished: false,
            }),
        }
    }

    fn sync_update(&mut self, frame: &Value) -> Result<Absorbed, String> {
        let state = frame
            .pointer("/state/runState/type")
            .and_then(Value::as_str);
        if state == Some("running") {
            self.seen_running = true;
        }
        if state == Some("idle") && self.seen_running {
            return Ok(Absorbed {
                chunks: vec![self.finish(ChatFinishReason::Stop)?, done()],
                finished: true,
            });
        }
        Ok(Absorbed {
            chunks: Vec::new(),
            finished: false,
        })
    }

    fn blocks_update(&mut self, frame: &Value) -> Result<Absorbed, String> {
        let mut blocks = frame
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        blocks.extend(
            frame
                .get("updates")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|updates| updates.values()),
        );
        let mut chunks = Vec::new();
        for block in blocks {
            let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(kind, "thinking" | "agent_content") {
                let Some(id) = block.get("blockId").and_then(Value::as_str) else {
                    continue;
                };
                let content = block.get("content").and_then(Value::as_str).unwrap_or("");
                let key = format!("{kind}:{id}");
                let delta = match self.previous.get(&key) {
                    Some(previous) => content.strip_prefix(previous).unwrap_or(""),
                    None => content,
                }
                .to_owned();
                self.previous.insert(key, content.to_owned());
                if !delta.is_empty() {
                    chunks.push(if kind == "thinking" {
                        self.text_delta(None, Some(delta))?
                    } else {
                        self.text_delta(Some(delta), None)?
                    });
                }
            } else if kind == "tool_use" && self.emit_tool_trace {
                self.tool_trace(block, &mut chunks)?;
            }
        }
        Ok(Absorbed {
            chunks,
            finished: false,
        })
    }

    fn tool_trace(&mut self, block: &Value, chunks: &mut Vec<Bytes>) -> Result<(), String> {
        let Some(id) = block.get("toolUseId").and_then(Value::as_str) else {
            return Ok(());
        };
        if !self.tools.insert(id.to_owned()) {
            return Ok(());
        }
        let name = block
            .get("displayName")
            .or_else(|| block.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("tool");
        chunks.push(self.text_delta(None, Some(format!("\n[Tasklet tool: {name}]\n")))?);
        Ok(())
    }

    fn text_delta(
        &self,
        content: Option<String>,
        reasoning_content: Option<String>,
    ) -> Result<Bytes, String> {
        self.delta(ChatDelta {
            role: None,
            content,
            reasoning_content,
            refusal: None,
            tool_calls: None,
            function_call: None,
            obfuscation: None,
            extra: BTreeMap::new(),
        })
    }

    fn delta(&self, delta: ChatDelta) -> Result<Bytes, String> {
        self.encode(vec![ChatChunkChoice {
            index: 0,
            delta,
            finish_reason: None,
            logprobs: None,
            extra: BTreeMap::new(),
        }])
    }

    pub fn tool_call(&self, name: String, payload: InvocationPayload) -> Result<Absorbed, String> {
        let (type_, function, custom) = match payload {
            InvocationPayload::Function(arguments) => (
                ChatToolCallType::Function,
                Some(FunctionCallDelta {
                    arguments: Some(arguments),
                    name: Some(name),
                    extra: BTreeMap::new(),
                }),
                None,
            ),
            InvocationPayload::Custom(input) => (
                ChatToolCallType::Custom,
                None,
                Some(CustomToolCallDelta {
                    input: Some(input),
                    name: Some(name),
                    extra: BTreeMap::new(),
                }),
            ),
        };
        let call = self.delta(ChatDelta {
            role: None,
            content: None,
            reasoning_content: None,
            refusal: None,
            tool_calls: Some(vec![ChatToolCallDelta {
                index: 0,
                id: Some(format!(
                    "call_{}",
                    crate::util::rand::uuid_v4().replace('-', "")
                )),
                type_: Some(type_),
                function,
                custom,
                extra: BTreeMap::new(),
            }]),
            function_call: None,
            obfuscation: None,
            extra: BTreeMap::new(),
        })?;
        Ok(Absorbed {
            chunks: vec![call, self.finish(ChatFinishReason::ToolCalls)?, done()],
            finished: true,
        })
    }

    fn finish(&self, reason: ChatFinishReason) -> Result<Bytes, String> {
        self.encode(vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: None,
                content: None,
                reasoning_content: None,
                refusal: None,
                tool_calls: None,
                function_call: None,
                obfuscation: None,
                extra: BTreeMap::new(),
            },
            finish_reason: Some(reason),
            logprobs: None,
            extra: BTreeMap::new(),
        }])
    }

    fn encode(&self, choices: Vec<ChatChunkChoice>) -> Result<Bytes, String> {
        let chunk = ChatCompletionChunk {
            id: self.id.clone(),
            choices,
            created: self.created,
            model: OpenAiModelId::from(self.model.clone()),
            object: ChatCompletionChunkObjectType::ChatCompletionChunk,
            service_tier: None,
            system_fingerprint: None,
            usage: None,
            extra: BTreeMap::new(),
        };
        let json = serde_json::to_string(&chunk)
            .map_err(|error| format!("tasklet OpenAI chunk JSON: {error}"))?;
        Ok(Bytes::from(SseFrame::data(json).encode()))
    }
}

fn done() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}
