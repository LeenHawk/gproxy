use super::super::ClaudeGeminiUsageFacts;
use super::{
    common::{Budget, StreamEnd, StreamLimits, bound, invalid, limit, response_id},
    usage::{GeminiUsageProgress, gemini_usage, prepare_initial},
};
use crate::transform::generate::stream::{
    claude::{ClaudeStreamCollector, ClaudeStreamLimits},
    gemini::{GeminiStreamCollector, GeminiStreamLimits},
};
use crate::{
    transform::{
        Converted, Report, TransformError, TransformErrorKind,
        generate::signature,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        claude::{
            content::{ThinkingBlock, ThinkingBlockType},
            generate_content as c, stream as s,
        },
        gemini as g,
    },
};

#[derive(Debug, Default)]
pub struct GeminiToClaudeContext {
    /// Actual model fallback, used only until a native model is observed.
    pub model: Option<String>,
    /// Actual source identity fallback; None permits a stable generated alias.
    pub response_id: Option<String>,
    pub usage: Option<c::Usage>,
    pub facts: ClaudeGeminiUsageFacts,
}

pub struct GeminiToClaudeStream {
    source: Option<GeminiStreamCollector>,
    target: Option<ClaudeStreamCollector>,
    flow: IdentityFlow,
    policy: TargetIdPolicy,
    budget: Budget,
    limits: StreamLimits,
    ctx: GeminiToClaudeContext,
    fixed_start: bool,
    usage: GeminiUsageProgress,
    source_id: Option<String>,
    model: Option<String>,
    started: bool,
    failed: bool,
    pending: Vec<s::StreamEvent>,
    pending_bytes: usize,
    next_block: usize,
    text_block: Option<i64>,
    /// The open thinking block, closed by its run's signature part.
    thinking_block: Option<i64>,

    calls: super::super::history::Calls,
}

impl GeminiToClaudeStream {
    pub fn new(
        ctx: GeminiToClaudeContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(ctx, flow, limits, super::common::claude_policy())
    }
    pub fn new_with_policy(
        mut ctx: GeminiToClaudeContext,
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

        ctx.usage = ctx.usage.into_declared();

        if ctx.model.as_ref().is_some_and(|v| v.trim().is_empty())
            || ctx.response_id.as_ref().is_some_and(String::is_empty)
        {
            return Err(invalid(
                "context",
                "empty actual model or response identity",
            ));
        }
        if let Some(usage) = &mut ctx.usage {
            prepare_initial(usage, ctx.facts)?;
        }
        bound(
            &(&ctx.model, &ctx.response_id, &ctx.usage),
            limits.max_bytes,
        )?;
        Ok(Self {
            source: Some(GeminiStreamCollector::new(GeminiStreamLimits {
                max_bytes: limits.max_bytes,
            })),
            target: Some(ClaudeStreamCollector::new(ClaudeStreamLimits {
                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            })),
            flow,
            policy,
            budget: Budget::new(limits),
            limits,
            source_id: ctx.response_id.clone(),
            model: ctx.model.clone(),
            fixed_start: ctx.usage.is_some(),
            ctx,
            usage: Default::default(),
            started: false,
            failed: false,
            pending: Vec::new(),
            pending_bytes: 0,
            next_block: 0,
            text_block: None,
            thinking_block: None,

            calls: Default::default(),
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        chunk: g::GenerateContentResponseBody,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        if self.failed {
            return Err(invalid("stream", "stream failed"));
        }
        let result = self.inner(chunk.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        chunk: g::GenerateContentResponseBody,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        self.budget.input(&chunk)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("stream", "source consumed"))?
            .push(chunk.clone())?;
        if let Some(id) = chunk.response_id {
            if self.started {
                response_id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::Gemini,
                    Some(id.clone()),
                )?;
            }
            self.source_id = Some(id);
        }
        if let Some(model) = chunk.model_version {
            if self.started && self.model.as_ref() != Some(&model) {
                return Err(invalid(
                    "model_version",
                    "actual model contradicts emitted Claude model",
                ));
            }
            self.model = Some(model);
        }
        if let Some(usage) = chunk.usage_metadata {
            self.usage.observe(&usage);
        }
        if self.ctx.usage.is_none() {
            let mut snapshot = self.usage.usage().clone();
            let mut current_facts = self.ctx.facts;
            current_facts.thinking_tokens = None;
            let observed = self
                .usage
                .effective(&mut snapshot, None)
                .and_then(|_| gemini_usage(snapshot, current_facts, None, &mut Report::default()));
            match observed {
                Ok((usage, _)) => self.ctx.usage = Some(usage),
                // Partial usage fields may precede a coherent total snapshot.
                // Native negative/decreasing counters already failed above.
                Err(e)
                    if matches!(
                        e.kind(),
                        TransformErrorKind::MissingMetadata | TransformErrorKind::InvalidResult
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        let mut out = Vec::new();
        let mut report = Report::default();
        self.ensure_start(&mut out)?;
        for candidate in chunk.candidates.unwrap_or_default() {
            if candidate.index.is_some_and(|v| v != 0) {
                continue;
            }
            if candidate
                .content
                .as_ref()
                .and_then(|v| v.role.as_deref())
                .is_some_and(|v| v != "model")
            {
                return Err(invalid("candidate.role", "model role required"));
            }
            for part in candidate.content.and_then(|v| v.parts).unwrap_or_default() {
                crate::transform::generate::gemini_controls::omitted(&part, &mut report);
                self.part(part, &mut out)?;
            }
        }
        Ok(Converted { value: out, report })
    }
    fn ensure_start(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<(), TransformError> {
        if self.started {
            return Ok(());
        }
        let (Some(model), Some(usage)) = (&self.model, &self.ctx.usage) else {
            return Ok(());
        };
        let id = response_id(
            &mut self.flow,
            &self.policy,
            crate::Dialect::Gemini,
            self.source_id.clone(),
        )?;
        let message = s::StreamMessage {
            input_transformations: None,
            type_: c::GenerateContentResponseBodyType::Tag,
            id,
            container: None,
            content: Vec::new(),
            context_management: None,
            diagnostics: None,
            model: model.clone(),
            role: c::ResponseRole::Assistant,
            stop_details: None,
            stop_reason: None,
            stop_sequence: None,
            usage: usage.clone(),
            rest: Default::default(),
        };
        let event =
            s::StreamEvent::MessageStart(Box::new(s::MessageStartEvent::builder(message).build()));
        self.budget.output(&event)?;
        self.deliver(out, event)?;
        self.started = true;
        for event in std::mem::take(&mut self.pending) {
            self.deliver(out, event)?;
        }
        self.pending_bytes = 0;
        Ok(())
    }
    fn allocate_block(&mut self) -> Result<i64, TransformError> {
        let index = i64::try_from(self.next_block).map_err(|_| limit())?;
        self.next_block += 1;
        Ok(index)
    }
    fn part(&mut self, part: g::Part, out: &mut Vec<s::StreamEvent>) -> Result<(), TransformError> {
        if super::super::thinking::is_empty_text(&part) {
            return Ok(());
        }
        if super::super::thinking::is_run_part(&part) {
            return self.thought(part, out);
        }
        self.close_thinking(out)?;
        let carried = super::super::thinking::call_signature(&part);
        if let Some(text) = part.text.filter(|_| part.thought != Some(true)) {
            // Gemini chunks extend the current text; they are not separate
            // Claude messages. In particular, a trailing empty chunk must not
            // replace the CLI's final assistant result with an empty block.
            let index = if let Some(index) = self.text_block {
                index
            } else {
                let index = self.allocate_block()?;
                let block = c::ResponseContentBlock::Text(
                    c::ResponseTextBlock::builder(c::ResponseTextBlockType::Tag, String::new())
                        .build(),
                );
                self.emit(
                    out,
                    s::StreamEvent::ContentBlockStart(
                        s::ContentBlockStartEvent::builder(index, block).build(),
                    ),
                )?;
                self.text_block = Some(index);
                index
            };
            self.emit(
                out,
                s::StreamEvent::ContentBlockDelta(
                    s::ContentBlockDeltaEvent::builder(
                        index,
                        s::ContentBlockDelta::Text(s::TextDelta::builder(text).build()),
                    )
                    .build(),
                ),
            )?;
        }
        if let Some(call) = part.function_call {
            self.close_text(out)?;

            if let Some(carried) = carried {
                // The call's own signature travels on an empty thinking block
                // right before it; see `thinking`.
                self.open_thinking(out)?;
                self.end_thinking(carried, out)?;
            }

            let id = self.calls.call(
                call.id,
                &call.name,
                crate::Dialect::Gemini,
                &mut self.flow,
                &self.policy,
            )?;
            let index = self.allocate_block()?;
            let block = c::ResponseContentBlock::ToolUse(
                c::ResponseToolUseBlock::builder(
                    c::ResponseToolUseBlockType::Tag,
                    id,
                    Default::default(),
                    call.name,
                )
                .build(),
            );
            self.emit(
                out,
                s::StreamEvent::ContentBlockStart(
                    s::ContentBlockStartEvent::builder(index, block).build(),
                ),
            )?;
            let json = serde_json::to_string(&call.args.unwrap_or_default())?;
            self.emit(
                out,
                s::StreamEvent::ContentBlockDelta(
                    s::ContentBlockDeltaEvent::builder(
                        index,
                        s::ContentBlockDelta::InputJson(s::InputJsonDelta::builder(json).build()),
                    )
                    .build(),
                ),
            )?;
            self.emit(
                out,
                s::StreamEvent::ContentBlockStop(s::ContentBlockStopEvent::builder(index).build()),
            )?;
        }
        Ok(())
    }
    /// Thought text streams as thinking deltas; the run's signature part
    /// sends its carried signature and closes the block.
    fn thought(
        &mut self,
        part: g::Part,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        self.close_text(out)?;
        let index = self.open_thinking(out)?;
        if let Some(text) = part.text.filter(|text| !text.is_empty()) {
            self.emit(
                out,
                s::StreamEvent::ContentBlockDelta(
                    s::ContentBlockDeltaEvent::builder(
                        index,
                        s::ContentBlockDelta::Thinking(s::ThinkingDelta::builder(text).build()),
                    )
                    .build(),
                ),
            )?;
        }
        if let Some(native) = part.thought_signature.as_deref() {
            self.end_thinking(signature::gemini_thought(Some(native)), out)?;
        }
        Ok(())
    }
    /// The open thinking block, started empty when none is open.
    fn open_thinking(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<i64, TransformError> {
        if let Some(index) = self.thinking_block {
            return Ok(index);
        }
        let index = self.allocate_block()?;
        let block = c::ResponseContentBlock::Thinking(
            ThinkingBlock::builder(ThinkingBlockType::Tag, String::new(), String::new()).build(),
        );
        self.emit(
            out,
            s::StreamEvent::ContentBlockStart(
                s::ContentBlockStartEvent::builder(index, block).build(),
            ),
        )?;
        self.thinking_block = Some(index);
        Ok(index)
    }
    /// A run that ends without a signature is an unsigned summary.
    fn close_thinking(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<(), TransformError> {
        if self.thinking_block.is_some() {
            self.end_thinking(signature::gemini_thought(None), out)?;
        }
        Ok(())
    }
    fn end_thinking(
        &mut self,
        signature: String,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        let Some(index) = self.thinking_block.take() else {
            return Ok(());
        };
        self.emit(
            out,
            s::StreamEvent::ContentBlockDelta(
                s::ContentBlockDeltaEvent::builder(
                    index,
                    s::ContentBlockDelta::Signature(s::SignatureDelta::builder(signature).build()),
                )
                .build(),
            ),
        )?;
        self.emit(
            out,
            s::StreamEvent::ContentBlockStop(s::ContentBlockStopEvent::builder(index).build()),
        )
    }
    fn close_text(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<(), TransformError> {
        if let Some(index) = self.text_block.take() {
            self.emit(
                out,
                s::StreamEvent::ContentBlockStop(s::ContentBlockStopEvent::builder(index).build()),
            )?;
        }
        Ok(())
    }
    fn emit(
        &mut self,
        out: &mut Vec<s::StreamEvent>,
        event: s::StreamEvent,
    ) -> Result<(), TransformError> {
        let size = self.budget.output(&event)?;
        if self.started {
            self.deliver(out, event)
        } else {
            let bytes = self
                .pending_bytes
                .checked_add(size)
                .filter(|n| *n <= self.limits.max_pending)
                .ok_or_else(limit)?;
            self.pending.push(event);
            self.pending_bytes = bytes;
            Ok(())
        }
    }
    fn deliver(
        &mut self,
        out: &mut Vec<s::StreamEvent>,
        event: s::StreamEvent,
    ) -> Result<(), TransformError> {
        self.target
            .as_mut()
            .ok_or_else(|| invalid("stream", "target consumed"))?
            .push(event.clone())?;
        out.push(event);
        Ok(())
    }
    pub fn finish(mut self) -> Result<StreamEnd<s::StreamEvent>, TransformError> {
        if self.failed {
            return Err(invalid("stream", "stream failed"));
        }
        let mut source = self
            .source
            .take()
            .ok_or_else(|| invalid("stream", "source consumed"))?
            .finish()?
            .value;
        let normalized = self.usage.effective(
            source
                .usage_metadata
                .as_mut()
                .ok_or_else(|| TransformError::missing_metadata("usage_metadata"))?,
            self.ctx.facts.thinking_tokens,
        )?;
        let (usage, facts) = gemini_usage(
            source
                .usage_metadata
                .clone()
                .ok_or_else(|| TransformError::missing_metadata("usage_metadata"))?,
            self.ctx.facts,
            self.ctx.usage.as_ref().filter(|_| self.fixed_start),
            &mut Report::default(),
        )?;

        response_id(
            &mut self.flow,
            &self.policy,
            crate::Dialect::Gemini,
            self.source_id.clone(),
        )?;
        let mut converted = super::super::gemini_to_claude_response(
            source,
            self.model.clone(),
            facts,
            &mut self.flow.clone(),
            &self.policy,
        )?;
        if normalized {
            converted.report.changed("usage.output_components", "stale prefix components resolved from current totals, components and explicit final facts");
        }
        let mut expected = converted.value;
        expected.usage = usage;
        bound(&expected, self.limits.max_bytes)?;
        let mut out = Vec::new();
        if !self.started {
            self.ctx.usage = Some(expected.usage.clone());
            self.model = Some(expected.model.clone());
            self.ensure_start(&mut out)?;
        }
        self.close_thinking(&mut out)?;
        self.close_text(&mut out)?;
        let delta = s::MessageDelta::builder()
            .stop_reason(Some(expected.stop_reason))
            .build();
        let usage = &expected.usage;
        let usage = s::MessageDeltaUsage {
            output_tokens: usage.output_tokens,
            input_tokens: Some(Some(usage.input_tokens)),
            cache_creation_input_tokens: usage.cache_creation_input_tokens,
            cache_read_input_tokens: usage.cache_read_input_tokens,
            output_tokens_details: usage.output_tokens_details.clone(),
            server_tool_use: usage.server_tool_use.clone(),
            iterations: usage.iterations.clone(),
            fallback_credit: usage.fallback_credit.clone(),
            rest: Default::default(),
        };
        self.emit(
            &mut out,
            s::StreamEvent::MessageDelta(Box::new(
                s::MessageDeltaEvent::builder(delta, usage).build(),
            )),
        )?;
        self.emit(
            &mut out,
            s::StreamEvent::MessageStop(s::MessageStopEvent::builder().build()),
        )?;
        let actual = self
            .target
            .take()
            .ok_or_else(|| invalid("stream", "target consumed"))?
            .finish()?
            .value;
        if actual != expected {
            return Err(invalid(
                "stream.parity",
                "native Claude output differs from completed Gemini conversion",
            ));
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report: converted.report,
        })
    }
}
