use super::{
    common::{Budget, StreamEnd, chat_finish, id},
    *,
};
use crate::transform::generate::reasoning_details as rd;
use crate::transform::generate::stream::{
    chat::{self, ChatStreamCollector, ChatStreamLimits},
    claude::{ClaudeStreamCollector, ClaudeStreamLimits},
};

#[derive(Debug, Clone, Copy)]
pub struct ClaudeToChatContext {
    pub created: i64,
}

enum Block {
    Omitted,
    Text {
        pending: Option<String>,
        closed: bool,
    },
    Reasoning(crate::wire::claude::content::ThinkingBlock),
    Tool {
        index: i64,
        has_arguments: bool,
    },
}

pub struct ClaudeToChatStream {
    source: Option<ClaudeStreamCollector>,
    target: Option<ChatStreamCollector>,
    policy: TargetIdPolicy,
    flow: IdentityFlow,
    budget: Budget,
    limits: StreamLimits,
    context: ClaudeToChatContext,
    blocks: BTreeMap<i64, Block>,
    text_order: std::collections::VecDeque<i64>,
    next_tool: usize,
    id: Option<String>,
    model: Option<String>,
    stopped: bool,
    failed: bool,
    report: Report,
}

impl ClaudeToChatStream {
    pub fn new(
        context: ClaudeToChatContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(
            context,
            flow,
            limits,
            TargetIdPolicy::new(crate::Dialect::OpenAiChat),
        )
    }
    pub fn new_with_policy(
        context: ClaudeToChatContext,
        flow: IdentityFlow,
        limits: StreamLimits,
        policy: TargetIdPolicy,
    ) -> Result<Self, TransformError> {
        if policy.dialect != crate::Dialect::OpenAiChat {
            return Err(TransformError::shape(
                "stream.policy",
                "target policy dialect mismatch",
            ));
        }

        if context.created < 0 {
            return Err(invalid("negative factual creation timestamp"));
        }
        // These IDs were already allocated under the selected policy. The
        // native target collector must preserve them while checking syntax;
        // preserve_source_ids=false must not trigger a second allocation.
        let mut collector_policy = policy.clone();
        collector_policy.preserve_source_ids = true;
        Ok(Self {
            target: Some(ChatStreamCollector::with_limits(
                IdentityFlow::new(flow.namespace()),
                collector_policy,
                ChatStreamLimits {
                    max_events: limits.max_events,
                    max_bytes: limits.max_bytes,
                    max_choices: 1,
                    max_tool_calls: limits.max_tools,
                },
            )),
            source: Some(ClaudeStreamCollector::new(ClaudeStreamLimits {
                max_events: limits.max_events,
                max_json_bytes: limits.max_bytes,
                max_text_bytes: limits.max_bytes,
                max_blocks: limits.max_blocks,
            })),
            policy,
            flow,
            budget: Budget::new(limits),
            limits,
            context,
            blocks: BTreeMap::new(),
            text_order: Default::default(),
            next_tool: 0,
            id: None,
            model: None,
            stopped: false,
            failed: false,
            report: Default::default(),
        })
    }
    pub(crate) fn reserve_external_ids(
        &mut self,
        role: crate::transform::identity::IdentityRole,
        ids: &std::collections::BTreeSet<String>,
        max: usize,
    ) -> Result<(), TransformError> {
        self.flow.reserve_external_ids(role, ids, max).map_err(|e| {
            TransformError::new(
                crate::transform::TransformErrorKind::Conflict,
                "fanout.ids",
                e.to_string(),
            )
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        event: s::StreamEvent,
    ) -> Result<Converted<Vec<q::ChatCompletionChunk>>, TransformError> {
        if self.failed || self.stopped {
            self.failed = true;
            return Err(invalid("event after message_stop or failed stream"));
        }
        let result = self.inner(event.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        event: s::StreamEvent,
    ) -> Result<Converted<Vec<q::ChatCompletionChunk>>, TransformError> {
        self.budget.input(&event)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(event.clone())?;
        let mut out = Vec::new();
        match event {
            s::StreamEvent::MessageStart(v) => {
                self.id = Some(id(
                    &mut self.flow,
                    &self.policy,
                    IdentityRole::Response,
                    crate::Dialect::Claude,
                    Some(v.message.id),
                    0,
                )?);
                self.model = Some(v.message.model);
                self.emit(
                    &mut out,
                    q::Delta::builder().role(q::DeltaRole::Assistant).build(),
                )?;
            }
            s::StreamEvent::ContentBlockStart(v) => {
                if self.blocks.len() >= self.limits.max_blocks {
                    return Err(limit());
                }
                match v.content_block {
                    c::ResponseContentBlock::Text(text) => {
                        self.blocks.insert(
                            v.index,
                            Block::Text {
                                pending: Some(text.text),
                                closed: false,
                            },
                        );
                        self.text_order.push_back(v.index);
                        self.flush_text(&mut out)?;
                        if text.citations.is_some() {
                            self.report.omitted(
                                "content.citations",
                                "Chat deltas have no typed citation field",
                            );
                        }
                    }
                    c::ResponseContentBlock::ToolUse(tool) => {
                        if self.next_tool >= self.limits.max_tools {
                            return Err(limit());
                        }
                        if tool.name.is_empty() {
                            return Err(TransformError::missing_metadata("tool.name"));
                        }
                        let index = i64::try_from(self.next_tool).map_err(|_| limit())?;
                        self.next_tool += 1;
                        let call_id = id(
                            &mut self.flow,
                            &self.policy,
                            IdentityRole::ToolCall,
                            crate::Dialect::Claude,
                            Some(tool.id),
                            index as u64,
                        )?;
                        let has_arguments = !tool.input.is_empty();
                        let mut function = q::DeltaFunctionCall::builder().name(tool.name).build();
                        if has_arguments {
                            function.arguments = Some(Some(serde_json::to_string(&tool.input)?));
                        }
                        let mut call = q::DeltaToolCall::builder(index).build();
                        call.id = Some(Some(call_id));
                        call.type_ = Some(Some(q::DeltaToolCallType::Function));
                        call.function = Some(Some(function));
                        self.blocks.insert(
                            v.index,
                            Block::Tool {
                                index,
                                has_arguments,
                            },
                        );
                        let mut delta = q::Delta::builder().build();
                        delta.tool_calls = Some(Some(vec![call]));
                        self.emit(&mut out, delta)?;
                        if tool.caller.is_some() {
                            self.report
                                .omitted("tool.caller", "Chat stream has no Claude caller field");
                        }
                    }
                    c::ResponseContentBlock::Thinking(block) => {
                        self.blocks.insert(v.index, Block::Reasoning(block.clone()));
                        if !block.thinking.is_empty() {
                            let mut delta = q::Delta::builder().build();
                            delta.reasoning_content = Some(Some(block.thinking));
                            self.emit(&mut out, delta)?;
                        }
                    }
                    c::ResponseContentBlock::RedactedThinking(block) => {
                        self.blocks.insert(v.index, Block::Omitted);
                        let mut delta = q::Delta::builder().build();
                        delta.reasoning_details =
                            Some(Some(vec![rd::from_redacted(&block, v.index)]));
                        self.emit(&mut out, delta)?;
                    }
                    _ => {
                        self.blocks.insert(v.index, Block::Omitted);
                        self.report
                            .omitted("content_block", "block has no target representation");
                    }
                }
            }
            s::StreamEvent::ContentBlockDelta(v) => {
                let index = v.index;
                if matches!(self.blocks.get(&index), Some(Block::Omitted)) {
                    return Ok(Converted {
                        value: out,
                        report: Report::default(),
                    });
                }
                match v.delta {
                    s::ContentBlockDelta::Text(text) => {
                        let Some(Block::Text { pending, .. }) = self.blocks.get_mut(&index) else {
                            return Err(invalid("text delta on nontext block"));
                        };
                        pending.get_or_insert_default().push_str(&text.text);
                        self.flush_text(&mut out)?;
                    }
                    s::ContentBlockDelta::InputJson(json) => {
                        let (tool, has) = match self.blocks.get_mut(&index) {
                            Some(Block::Tool {
                                index,
                                has_arguments,
                            }) => (*index, has_arguments),
                            _ => return Err(invalid("JSON delta on nonclient tool")),
                        };
                        *has |= !json.partial_json.is_empty();
                        self.emit(&mut out, arguments(tool, json.partial_json))?;
                    }
                    s::ContentBlockDelta::Thinking(block) => {
                        let Some(Block::Reasoning(original)) = self.blocks.get_mut(&index) else {
                            return Err(invalid("thinking delta on nonreasoning block"));
                        };
                        original.thinking.push_str(&block.thinking);
                        let mut delta = q::Delta::builder().build();
                        delta.reasoning_content = Some(Some(block.thinking));
                        self.emit(&mut out, delta)?;
                    }
                    s::ContentBlockDelta::Signature(signature) => {
                        let Some(Block::Reasoning(original)) = self.blocks.get_mut(&index) else {
                            return Err(invalid("signature on nonreasoning block"));
                        };
                        original.signature = signature.signature;
                    }
                    s::ContentBlockDelta::Citations(_) => self
                        .report
                        .omitted("content.citations", "Chat has no typed citation delta"),
                    s::ContentBlockDelta::Compaction(_) => {
                        self.report
                            .omitted("content.compaction", "block has no target representation");
                    }
                }
            }
            s::StreamEvent::ContentBlockStop(v) => {
                if let Some(Block::Reasoning(block)) = self.blocks.get(&v.index) {
                    let mut delta = q::Delta::builder().build();
                    delta.reasoning_details = Some(Some(vec![rd::from_thinking(block, v.index)]));
                    self.emit(&mut out, delta)?;
                }
                if let Some(Block::Text { closed, .. }) = self.blocks.get_mut(&v.index) {
                    *closed = true;
                    self.flush_text(&mut out)?;
                }
                if let Some(Block::Tool {
                    index,
                    has_arguments: false,
                }) = self.blocks.get(&v.index)
                {
                    let delta = arguments(*index, "{}".into());
                    self.emit(&mut out, delta)?;
                }
            }
            s::StreamEvent::MessageDelta(v) => {
                if let Some(Some(reason)) = v.delta.stop_reason {
                    chat_finish(reason)?;
                }
            }
            s::StreamEvent::MessageStop(_) => self.stopped = true,
            s::StreamEvent::Ping(_) => {}
            s::StreamEvent::Error(_) => return Err(invalid("upstream Claude error")),
        }
        Ok(Converted {
            value: out,
            report: std::mem::take(&mut self.report),
        })
    }
    fn emit(
        &mut self,
        out: &mut Vec<q::ChatCompletionChunk>,
        delta: q::Delta,
    ) -> Result<(), TransformError> {
        let chunk = self.chunk(vec![q::StreamChoice::builder(0, delta).build()])?;
        self.emit_chunk(out, chunk)
    }
    fn emit_chunk(
        &mut self,
        out: &mut Vec<q::ChatCompletionChunk>,
        chunk: q::ChatCompletionChunk,
    ) -> Result<(), TransformError> {
        self.budget.emit(out, chunk)?;
        self.target
            .as_mut()
            .ok_or_else(|| invalid("target consumed"))?
            .push(out.last().unwrap().clone())?;
        Ok(())
    }
    // Claude block order is semantically significant. Later text blocks wait
    // only while an earlier text block remains open; the first block stays live.
    fn flush_text(&mut self, out: &mut Vec<q::ChatCompletionChunk>) -> Result<(), TransformError> {
        while let Some(index) = self.text_order.front().copied() {
            let (text, closed) = match self.blocks.get_mut(&index) {
                Some(Block::Text { pending, closed }) => (pending.take(), *closed),
                _ => return Err(invalid("missing ordered text block")),
            };
            if let Some(text) = text {
                self.emit(out, q::Delta::builder().content(text).build())?;
            }
            if !closed {
                break;
            }
            self.text_order.pop_front();
        }
        Ok(())
    }
    fn chunk(
        &self,
        choices: Vec<q::StreamChoice>,
    ) -> Result<q::ChatCompletionChunk, TransformError> {
        Ok(q::ChatCompletionChunk::builder(
            self.id.clone().ok_or_else(|| invalid("missing start ID"))?,
            choices,
            self.context.created,
            self.model.clone().ok_or_else(|| invalid("missing model"))?,
            q::ChunkObject::ChatCompletionChunk,
        )
        .build())
    }
    pub fn finish(mut self) -> Result<StreamEnd<q::ChatCompletionChunk>, TransformError> {
        if self.failed || !self.stopped {
            return Err(invalid("Claude stream ended before valid message_stop"));
        }
        let source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?
            .value;
        let finish = chat_finish(source.stop_reason)?;
        if source.stop_reason == c::StopReason::Refusal {
            self.report.changed("refusal","Claude refusal is learned after text delivery; streamed text remains content and terminal is content_filter");
        }
        let usage = super::super::usage::to_chat(&source.usage, &mut self.report)?;
        let mut out = Vec::new();
        let choice = q::StreamChoice::builder(0, q::Delta::builder().build())
            .finish_reason(finish)
            .build();
        let chunk = self.chunk(vec![choice])?;
        self.emit_chunk(&mut out, chunk)?;
        let mut tail = self.chunk(Vec::new())?;
        tail.usage = Some(Some(chat::usage::synthesize(usage)?));
        self.emit_chunk(&mut out, tail)?;
        let mut target = self
            .target
            .take()
            .ok_or_else(|| invalid("target consumed"))?;
        target.push_done()?;
        target.finish()?;
        for (present, field) in [
            (source.container.is_some(), "container"),
            (source.context_management.is_some(), "context_management"),
            (source.diagnostics.is_some(), "diagnostics"),
            (source.stop_details.is_some(), "stop_details"),
            (source.stop_sequence.is_some(), "stop_sequence"),
        ] {
            if present {
                self.report
                    .omitted(field, "Chat stream has no equivalent Claude metadata field");
            }
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report: self.report,
        })
    }
}

fn arguments(index: i64, text: String) -> q::Delta {
    let mut tool = q::DeltaToolCall::builder(index).build();
    tool.function = Some(Some(
        q::DeltaFunctionCall::builder().arguments(text).build(),
    ));
    let mut delta = q::Delta::builder().build();
    delta.tool_calls = Some(Some(vec![tool]));
    delta
}
