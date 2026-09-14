use super::{
    chat_blocks::{self, Tool},
    common::{Budget, StreamEnd, bounded, claude_finish, id},
    *,
};
use crate::transform::generate::stream::{
    chat::{self, ChatStreamCollector, ChatStreamLimits},
    claude::{ClaudeStreamCollector, ClaudeStreamLimits},
};
/// Claude needs real initial usage. Without this fact, pending Chat chunks are
/// released only after an actual usage record arrives. The source must supply
/// its actual response model; no model name is guessed.
#[derive(Debug, Clone, Default)]
pub struct ChatToClaudeContext {
    pub start_usage: Option<c::Usage>,
}
pub struct ChatToClaudeStream {
    source: Option<ChatStreamCollector>,
    target: Option<ClaudeStreamCollector>,
    policy: TargetIdPolicy,
    flow: IdentityFlow,
    budget: Budget,
    limits: StreamLimits,
    context: ChatToClaudeContext,
    pending: Vec<q::ChatCompletionChunk>,
    source_id: Option<String>,
    model: Option<String>,
    started: bool,
    done: bool,
    failed: bool,
    closed_blocks: bool,
    text: Option<i64>,
    refusal_text: Option<String>,
    next_block: i64,
    tools: BTreeMap<i64, Tool>,
    legacy: Option<Tool>,
    tool_count: usize,
    report: Report,
}
impl ChatToClaudeStream {
    pub fn new(
        context: ChatToClaudeContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(context, flow, limits, super::common::claude_policy())
    }
    pub fn new_with_policy(
        mut context: ChatToClaudeContext,
        flow: IdentityFlow,
        limits: StreamLimits,
        policy: TargetIdPolicy,
    ) -> Result<Self, TransformError> {
        if policy.dialect != crate::Dialect::Claude {
            return Err(TransformError::shape(
                "stream.policy",
                "target policy dialect mismatch",
            ));
        }

        context.start_usage = context.start_usage.into_declared();
        if let Some(usage) = &context.start_usage {
            bounded(usage, limits.max_bytes)?;
            super::super::usage::to_chat(usage, &mut Report::default())?;
        }
        let native = ClaudeStreamLimits {
            max_events: limits.max_events,
            max_json_bytes: limits.max_bytes,
            max_text_bytes: limits.max_bytes,
            max_blocks: limits.max_blocks,
        };
        Ok(Self {
            source: Some(ChatStreamCollector::with_limits(
                IdentityFlow::new(flow.namespace()),
                TargetIdPolicy::new(crate::Dialect::OpenAiChat),
                ChatStreamLimits {
                    max_events: limits.max_events,
                    max_bytes: limits.max_bytes,
                    max_choices: 1,
                    max_tool_calls: limits.max_tools,
                },
            )),
            target: Some(ClaudeStreamCollector::new(native)),
            policy,
            flow,
            budget: Budget::new(limits),
            limits,
            model: None,
            context,
            pending: Vec::new(),
            source_id: None,
            started: false,
            done: false,
            failed: false,
            closed_blocks: false,
            text: None,
            refusal_text: None,
            next_block: 0,
            tools: BTreeMap::new(),
            legacy: None,
            tool_count: 0,
            report: Default::default(),
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        chunk: q::ChatCompletionChunk,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        if self.failed || self.done {
            self.failed = true;
            return Err(invalid("event after DONE or failed stream"));
        }
        let result = self.inner(chunk.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        chunk: q::ChatCompletionChunk,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        self.budget.input(&chunk)?;
        if chunk.choices.iter().any(|v| v.index != 0) {
            return Err(TransformError::unsupported(
                "choices",
                "one Claude message requires one Chat choice; fanout needs separate invocations",
            ));
        }
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(chunk.clone())?;
        if !chunk.id.is_empty() {
            self.source_id = Some(chunk.id.clone());
        }
        if !chunk.model.is_empty() {
            if self.started && self.model.as_ref().is_some_and(|v| v != &chunk.model) {
                return Err(invalid("model changed after emission"));
            }
            self.model = Some(chunk.model.clone());
        }
        if self.context.start_usage.is_none()
            && let Some(usage) = chunk.usage.as_ref().and_then(Option::as_ref)
        {
            let usage = chat::usage::collect(usage.clone(), &mut self.report);
            self.context.start_usage =
                Some(super::super::usage::to_claude(&usage, &mut self.report)?);
        }
        if self.context.start_usage.is_none() || self.model.is_none() {
            self.pending.push(chunk);
            return Ok(Converted {
                value: Vec::new(),
                report: Report::default(),
            });
        }
        let mut out = Vec::new();
        if !self.started {
            self.start(&mut out)?;
        }
        for chunk in std::mem::take(&mut self.pending) {
            self.content(chunk, &mut out)?;
        }
        self.content(chunk, &mut out)?;
        Ok(Converted {
            value: out,
            report: std::mem::take(&mut self.report),
        })
    }
    fn start(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<(), TransformError> {
        let id = id(
            &mut self.flow,
            &self.policy,
            IdentityRole::Response,
            crate::Dialect::OpenAiChat,
            self.source_id.clone(),
            0,
        )?;
        let mut initial_usage = self
            .context
            .start_usage
            .clone()
            .ok_or_else(|| TransformError::missing_metadata("initial usage"))?;
        if initial_usage.output_tokens_details.take().is_some() {
            self.report.changed(
                "usage.output_tokens_details",
                "initial output detail is deferred until a final source counter is available",
            );
        }
        let message = s::StreamMessage {
            type_: c::GenerateContentResponseBodyType::Tag,
            id,
            container: None,
            content: Vec::new(),
            context_management: None,
            diagnostics: None,
            model: self
                .model
                .clone()
                .ok_or_else(|| TransformError::missing_metadata("model"))?,
            role: c::ResponseRole::Assistant,
            stop_details: None,
            stop_reason: None,
            stop_sequence: None,
            usage: initial_usage,
            rest: Default::default(),
        };
        self.emit(
            out,
            s::StreamEvent::MessageStart(Box::new(s::MessageStartEvent::builder(message).build())),
        )?;
        self.started = true;
        Ok(())
    }
    fn content(
        &mut self,
        chunk: q::ChatCompletionChunk,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        if let Some(id) = self.source_id.clone() {
            common::id(
                &mut self.flow,
                &self.policy,
                IdentityRole::Response,
                crate::Dialect::OpenAiChat,
                Some(id),
                0,
            )?;
        }
        for choice in chunk.choices {
            let delta = choice.delta;
            if let Some(text) = delta.content.flatten() {
                let index = self.text_block(out)?;
                self.emit(out, chat_blocks::text_delta(index, text))?;
            }
            if let Some(refusal) = delta.refusal.flatten() {
                self.refusal_text.get_or_insert_default().push_str(&refusal);
            }
            for call in delta.tool_calls.flatten().into_iter().flatten() {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    self.tools.entry(call.index)
                {
                    if self.tool_count >= self.limits.max_tools {
                        return Err(limit());
                    }
                    self.tool_count += 1;
                    entry.insert(Tool::default());
                }
                self.tools
                    .get_mut(&call.index)
                    .unwrap()
                    .append(call.id.flatten(), call.function.flatten())?;
            }
            if let Some(function) = delta.function_call.flatten() {
                if self.legacy.is_none() {
                    if self.tool_count >= self.limits.max_tools {
                        return Err(limit());
                    }
                    self.tool_count += 1;
                    self.legacy = Some(Tool::default());
                }
                self.legacy.as_mut().unwrap().append(None, Some(function))?;
            }
            if choice.finish_reason.flatten().is_some() {
                self.close_blocks(out)?;
            }
        }
        Ok(())
    }
    fn text_block(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<i64, TransformError> {
        if let Some(index) = self.text {
            return Ok(index);
        }
        let index = self.allocate_block()?;
        self.emit(out, chat_blocks::text_start(index))?;
        self.text = Some(index);
        Ok(index)
    }
    fn allocate_block(&mut self) -> Result<i64, TransformError> {
        if self.next_block as usize >= self.limits.max_blocks {
            return Err(limit());
        }
        let index = self.next_block;
        self.next_block = self.next_block.checked_add(1).ok_or_else(limit)?;
        Ok(index)
    }
    fn close_blocks(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<(), TransformError> {
        if self.closed_blocks {
            return Err(invalid("duplicate choice finish"));
        }
        if let Some(index) = self.text {
            self.emit(out, chat_blocks::block_stop(index))?;
        }
        if let Some(text) = self.refusal_text.take() {
            let index = self.allocate_block()?;
            self.emit(out, chat_blocks::text_start(index))?;
            self.emit(out, chat_blocks::text_delta(index, text))?;
            self.emit(out, chat_blocks::block_stop(index))?;
        }
        let mut calls: Vec<_> = std::mem::take(&mut self.tools).into_values().collect();
        if let Some(tool) = self.legacy.take() {
            calls.push(tool);
        }
        for (ordinal, tool) in calls.into_iter().enumerate() {
            tool.object()?;
            let block = self.allocate_block()?;
            let id = id(
                &mut self.flow,
                &self.policy,
                IdentityRole::ToolCall,
                crate::Dialect::OpenAiChat,
                tool.id,
                ordinal as u64,
            )?;
            let content = c::ResponseContentBlock::ToolUse(
                c::ResponseToolUseBlock::builder(
                    c::ResponseToolUseBlockType::Tag,
                    id,
                    Default::default(),
                    tool.name,
                )
                .build(),
            );
            self.emit(
                out,
                s::StreamEvent::ContentBlockStart(
                    s::ContentBlockStartEvent::builder(block, content).build(),
                ),
            )?;
            self.emit(
                out,
                s::StreamEvent::ContentBlockDelta(
                    s::ContentBlockDeltaEvent::builder(
                        block,
                        s::ContentBlockDelta::InputJson(
                            s::InputJsonDelta::builder(tool.arguments).build(),
                        ),
                    )
                    .build(),
                ),
            )?;
            self.emit(out, chat_blocks::block_stop(block))?;
        }
        self.closed_blocks = true;
        Ok(())
    }
    fn emit(
        &mut self,
        out: &mut Vec<s::StreamEvent>,
        event: s::StreamEvent,
    ) -> Result<(), TransformError> {
        self.budget.emit(out, event)?;
        self.target
            .as_mut()
            .ok_or_else(|| invalid("target consumed"))?
            .push(out.last().unwrap().clone())?;
        Ok(())
    }
    pub fn push_done(&mut self) -> Result<(), TransformError> {
        if self.failed || self.done {
            self.failed = true;
            return Err(invalid("duplicate DONE or failed stream"));
        }
        let result = self
            .source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push_done();
        if result.is_err() {
            self.failed = true;
        } else {
            self.done = true;
        }
        result
    }
    pub fn finish(self) -> Result<StreamEnd<s::StreamEvent>, TransformError> {
        self.finish_with_usage(None)
    }
    /// Actual final usage may be supplied when the Chat provider omits usage.
    pub fn finish_with_usage(
        mut self,
        facts: Option<c::Usage>,
    ) -> Result<StreamEnd<s::StreamEvent>, TransformError> {
        let facts = facts.into_declared();
        if self.failed || !self.done {
            return Err(invalid("Chat stream lacks valid DONE"));
        }
        let source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?;
        self.report.diagnostics.extend(source.report.diagnostics);
        let source = source.value;
        if let Some(facts) = &facts {
            bounded(facts, self.limits.max_bytes)?;
        }
        let usage = super::usage_facts::resolve(
            source.usage.as_ref(),
            facts,
            self.context.start_usage.as_ref(),
            &mut self.report,
        )?;
        bounded(&usage, self.limits.max_bytes)?;
        let mut out = Vec::new();
        if !self.started {
            self.context.start_usage = Some(usage.clone());
            self.model = Some(source.model.clone());
            self.source_id = Some(source.id.clone());
            self.start(&mut out)?;
            for chunk in std::mem::take(&mut self.pending) {
                self.content(chunk, &mut out)?;
            }
        }
        if !self.closed_blocks {
            self.close_blocks(&mut out)?;
        }
        let choice = source
            .choices
            .first()
            .ok_or_else(|| invalid("missing choice"))?;
        if let Some(initial) = &self.context.start_usage {
            for (changed, field) in [
                (
                    usage.cache_creation.is_some()
                        && usage.cache_creation != initial.cache_creation,
                    "cache_creation",
                ),
                (
                    usage.service_tier.is_some() && usage.service_tier != initial.service_tier,
                    "service_tier",
                ),
                (
                    usage.inference_geo.is_some() && usage.inference_geo != initial.inference_geo,
                    "inference_geo",
                ),
                (
                    usage.speed.is_some() && usage.speed != initial.speed,
                    "speed",
                ),
            ] {
                if changed {
                    self.report.omitted(
                        format!("usage.{field}"),
                        "Claude message_delta has no field for this late usage metadata",
                    );
                }
            }
        }
        let delta = s::MessageDelta::builder()
            .stop_reason(Some(claude_finish(
                choice.finish_reason,
                choice.message.refusal.is_some(),
            )))
            .build();
        let event = s::StreamEvent::MessageDelta(Box::new(
            s::MessageDeltaEvent::builder(delta, chat_blocks::usage_delta(usage)).build(),
        ));
        self.emit(&mut out, event)?;
        self.emit(
            &mut out,
            s::StreamEvent::MessageStop(s::MessageStopEvent::builder().build()),
        )?;
        self.target
            .take()
            .ok_or_else(|| invalid("target consumed"))?
            .finish()?;
        for (present, field) in [
            (choice.logprobs.is_some(), "logprobs"),
            (source.service_tier.is_some(), "service_tier"),
            (source.system_fingerprint.is_some(), "system_fingerprint"),
            (source.moderation.is_some(), "moderation"),
        ] {
            if present {
                self.report
                    .omitted(field, "Claude stream has no equivalent Chat metadata field");
            }
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report: self.report,
        })
    }
}
