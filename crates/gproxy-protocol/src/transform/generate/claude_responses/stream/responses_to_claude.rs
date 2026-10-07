use super::{
    common::{
        Budget, StreamEnd, StreamLimits, declared, id, initial_usage, invalid, limit, measure,
    },
    response_items::Item,
    usage::claude_tier,
};
use crate::{
    transform::generate::stream::{
        claude::{ClaudeStreamCollector, ClaudeStreamLimits},
        responses::{ResponsesStreamCollector, ResponsesStreamLimits},
    },
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, TargetIdPolicy},
    },
    wire::{
        claude::{generate_content as c, stream as cs},
        openai::responses::{response as r, stream as s},
    },
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct ResponsesToClaudeContext {
    pub usage: Option<c::Usage>,
}

pub struct ResponsesToClaudeStream {
    source: Option<ResponsesStreamCollector>,
    target: Option<ClaudeStreamCollector>,
    pub(super) flow: IdentityFlow,
    pub(super) policy: TargetIdPolicy,
    pub(super) limits: StreamLimits,
    budget: Budget,
    usage: Option<c::Usage>,
    observed_usage: Option<r::ResponseUsage>,
    pub(super) model: Option<String>,
    response_id: Option<String>,
    tier: Option<Option<c::ResponseServiceTier>>,
    started: bool,
    failed: bool,
    terminal: bool,
    pending: Vec<cs::StreamEvent>,
    pending_bytes: usize,
    pub(super) held_bytes: usize,
    pub(super) items: BTreeMap<i64, Item>,
    pub(super) cursor: i64,

    next_block: usize,
    pub(super) report: Report,
}

impl ResponsesToClaudeStream {
    pub fn new(
        context: ResponsesToClaudeContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(context, flow, limits, super::common::claude_policy())
    }
    pub fn new_with_policy(
        mut context: ResponsesToClaudeContext,
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

        context.usage = context
            .usage
            .map(initial_usage)
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        measure(&context.usage, limits.max_bytes)?;
        Ok(Self {
            source: Some(ResponsesStreamCollector::new(ResponsesStreamLimits {
                max_bytes: limits.max_bytes,

                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            })),
            target: Some(ClaudeStreamCollector::new(ClaudeStreamLimits {
                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            })),
            flow,
            policy,
            limits,
            budget: Budget::new(limits),
            usage: context.usage,
            observed_usage: None,
            model: None,
            response_id: None,
            tier: None,
            started: false,
            failed: false,
            terminal: false,
            pending: Vec::new(),
            pending_bytes: 0,
            held_bytes: 0,
            items: BTreeMap::new(),
            cursor: 0,

            next_block: 0,
            report: Default::default(),
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        event: s::StreamEvent,
    ) -> Result<Converted<Vec<cs::StreamEvent>>, TransformError> {
        if self.failed || self.terminal {
            self.failed = true;
            return Err(invalid("event after terminal or failure"));
        }
        let result = self.inner(declared(event));
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        event: s::StreamEvent,
    ) -> Result<Converted<Vec<cs::StreamEvent>>, TransformError> {
        let event = crate::transform::generate::multi_agent::attribute_event(event)?;
        self.budget.input(&event)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(event.clone())?;
        let mut out = Vec::new();
        match event {
            s::StreamEvent::Keepalive => out.push(cs::StreamEvent::Ping(cs::PingEvent {
                rest: Default::default(),
            })),
            s::StreamEvent::Created(v) => {
                self.response_id = Some(id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::OpenAi,
                    IdentityRole::Response,
                    IdentityRole::Response,
                    Some(v.response.id.clone()),
                    0,
                )?);
                self.model = Some(v.response.model.clone());
                self.observe(&v.response)?;
                self.start(&mut out)?;
                for (index, item) in v.response.output.into_iter().enumerate() {
                    self.add_item(index as i64, item)?;
                }
            }
            s::StreamEvent::Queued(v) => {
                self.observe(&v.response)?;
                self.start(&mut out)?;
            }
            s::StreamEvent::InProgress(v) => {
                self.observe(&v.response)?;
                self.start(&mut out)?;
            }
            s::StreamEvent::OutputItemAdded(v) => self.add_item(v.output_index, v.item)?,
            s::StreamEvent::ContentPartAdded(v) => {
                self.bind(v.output_index, Some(&v.item_id))?;
                self.add_part(v.output_index, v.content_index, v.part)?;
            }
            s::StreamEvent::ContentPartDone(v) => {
                self.part_done(v.output_index, v.content_index, &mut out)?
            }
            s::StreamEvent::OutputTextDelta(v) => self.append_text(
                v.output_index,
                v.content_index,
                &v.item_id,
                v.delta,
                false,
                &mut out,
            )?,
            s::StreamEvent::RefusalDelta(v) => self.append_text(
                v.output_index,
                v.content_index,
                &v.item_id,
                v.delta,
                true,
                &mut out,
            )?,
            s::StreamEvent::FunctionCallArgumentsDelta(v) => {
                self.append_args(v.output_index, &v.item_id, v.delta, &mut out)?
            }
            s::StreamEvent::FunctionCallArgumentsDone(v) => {
                self.bind(v.output_index, Some(&v.item_id))?;
                self.args_done(v.output_index, &v.arguments, &mut out)?;
            }
            s::StreamEvent::OutputItemDone(v) => {
                self.item_done(v.output_index, v.item, &mut out)?
            }
            s::StreamEvent::Completed(v) => {
                self.observe(&v.response)?;
                self.terminal = true;
                self.start(&mut out)?;
            }
            s::StreamEvent::Incomplete(v) => {
                self.observe(&v.response)?;
                self.terminal = true;
                self.start(&mut out)?;
            }
            s::StreamEvent::ReasoningSummaryPartAdded(_) => {}
            s::StreamEvent::OutputTextDone(_)
            | s::StreamEvent::RefusalDone(_)
            | s::StreamEvent::OutputTextAnnotationAdded(_)
            | s::StreamEvent::ReasoningTextDelta(_)
            | s::StreamEvent::ReasoningTextDone(_)
            | s::StreamEvent::ReasoningSummaryTextDelta(_)
            | s::StreamEvent::ReasoningSummaryTextDone(_)
            | s::StreamEvent::ReasoningSummaryPartDone(_)
            | s::StreamEvent::McpArgumentsDelta(_)
            | s::StreamEvent::McpArgumentsDone(_)
            | s::StreamEvent::McpInProgress(_)
            | s::StreamEvent::McpCompleted(_)
            | s::StreamEvent::McpFailed(_) => {}
            s::StreamEvent::Failed(_) | s::StreamEvent::Error(_) => {
                return Err(invalid("native source failure"));
            }
            _ => {}
        }
        self.flush_items(&mut out)?;
        Ok(Converted {
            value: out,
            report: std::mem::take(&mut self.report),
        })
    }
    fn observe(&mut self, response: &r::GenerateContentResponseBody) -> Result<(), TransformError> {
        if !self.started {
            self.tier = claude_tier(response.service_tier.flatten()).or(self.tier);
        }
        if let Some(usage) = response.usage.as_ref().and_then(Option::as_ref) {
            let native = super::super::response::usage::to_claude(usage.clone())?;
            if let Some(previous) = &self.observed_usage
                && (usage.input_tokens < previous.input_tokens
                    || usage.output_tokens < previous.output_tokens
                    || usage.total_tokens < previous.total_tokens
                    || usage.input_tokens_details.cached_tokens
                        < previous.input_tokens_details.cached_tokens
                    || usage.input_tokens_details.cache_write_tokens
                        < previous.input_tokens_details.cache_write_tokens
                    || usage.output_tokens_details.reasoning_tokens
                        < previous.output_tokens_details.reasoning_tokens)
            {
                return Err(invalid("native cumulative usage decreased"));
            }
            self.observed_usage = Some(usage.clone());
            if self.usage.is_none() {
                self.usage = Some(native);
            }
        }
        Ok(())
    }
    fn start(&mut self, out: &mut Vec<cs::StreamEvent>) -> Result<(), TransformError> {
        if self.started {
            return Ok(());
        }
        let (Some(id), Some(model), Some(usage)) =
            (&self.response_id, &self.model, &mut self.usage)
        else {
            return Ok(());
        };
        if let Some(tier) = self.tier {
            if usage
                .service_tier
                .flatten()
                .zip(tier)
                .is_some_and(|(old, new)| old != new)
            {
                return Err(invalid("initial service-tier facts contradict source"));
            }
            usage.service_tier = Some(tier);
        }
        let message = cs::StreamMessage {
            input_transformations: None,
            type_: c::GenerateContentResponseBodyType::Tag,
            id: id.clone(),
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
        let event = cs::StreamEvent::MessageStart(Box::new(
            cs::MessageStartEvent::builder(message).build(),
        ));
        self.budget.output(&event)?;
        self.deliver(out, event)?;
        self.started = true;
        for event in std::mem::take(&mut self.pending) {
            self.deliver(out, event)?;
        }
        self.pending_bytes = 0;
        Ok(())
    }
    pub(super) fn reserve_held(&mut self, bytes: usize) -> Result<(), TransformError> {
        let held = self.held_bytes.checked_add(bytes).ok_or_else(limit)?;
        if held
            .checked_add(self.pending_bytes)
            .is_none_or(|n| n > self.limits.max_pending)
        {
            return Err(limit());
        }
        self.held_bytes = held;
        Ok(())
    }
    pub(super) fn allocate_block(&mut self) -> Result<i64, TransformError> {
        let index = i64::try_from(self.next_block).map_err(|_| limit())?;
        self.next_block += 1;
        Ok(index)
    }
    pub(super) fn complete_block(
        &mut self,
        out: &mut Vec<cs::StreamEvent>,
        block: c::ResponseContentBlock,
    ) -> Result<(), TransformError> {
        let index = self.allocate_block()?;
        self.emit(
            out,
            cs::StreamEvent::ContentBlockStart(
                cs::ContentBlockStartEvent::builder(index, block).build(),
            ),
        )?;
        self.emit(
            out,
            cs::StreamEvent::ContentBlockStop(cs::ContentBlockStopEvent::builder(index).build()),
        )
    }
    pub(super) fn emit(
        &mut self,
        out: &mut Vec<cs::StreamEvent>,
        event: cs::StreamEvent,
    ) -> Result<(), TransformError> {
        let n = self.budget.output(&event)?;
        if self.started {
            self.deliver(out, event)
        } else {
            let bytes = self.pending_bytes.checked_add(n).ok_or_else(limit)?;
            if bytes
                .checked_add(self.held_bytes)
                .is_none_or(|n| n > self.limits.max_pending)
            {
                return Err(limit());
            }
            self.pending.push(event);
            self.pending_bytes = bytes;
            Ok(())
        }
    }
    fn deliver(
        &mut self,
        out: &mut Vec<cs::StreamEvent>,
        event: cs::StreamEvent,
    ) -> Result<(), TransformError> {
        self.target
            .as_mut()
            .ok_or_else(|| invalid("target consumed"))?
            .push(event.clone())?;
        out.push(event);
        Ok(())
    }
    pub fn finish(mut self) -> Result<StreamEnd<cs::StreamEvent>, TransformError> {
        if self.failed || !self.terminal {
            return Err(invalid("failed stream or premature EOF"));
        }
        let source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?
            .value;
        let part_refusal = source.output.iter().any(|item| matches!(item, r::ResponseOutputItem::Message(message) if message.content.iter().any(|part| matches!(part, crate::wire::openai::responses::input::OutputContent::Refusal(_)))));
        let converted = super::super::responses_to_claude_response(
            source,
            &mut self.flow.clone(),
            &self.policy,
        )?;
        let mut expected = converted.value;
        if part_refusal && expected.stop_reason != c::StopReason::Refusal {
            self.report.changed("output.refusal", "refusal text is preserved; Claude has no separate per-part refusal marker alongside tool continuation or an incomplete stop");
        }
        self.report.diagnostics.extend(converted.report.diagnostics);
        let mut out = Vec::new();
        if !self.started {
            self.usage = Some(expected.usage.clone());
            self.start(&mut out)?;
        }
        let initial = self
            .usage
            .as_ref()
            .ok_or_else(|| invalid("missing start usage"))?;

        if expected.usage.service_tier != initial.service_tier {
            self.report.omitted(
                "usage.service_tier",
                "late effective tier cannot update Claude message_start",
            );
        }
        expected.usage.service_tier = initial.service_tier;
        expected.usage.cache_creation = initial.cache_creation.clone();
        expected.usage.fallback_credit = initial.fallback_credit.clone();
        expected.usage.inference_geo = initial.inference_geo.clone();
        expected.usage.iterations = initial.iterations.clone();
        expected.usage.server_tool_use = initial.server_tool_use.clone();
        expected.usage.speed = initial.speed;
        measure(&expected, self.limits.max_bytes)?;
        let usage = &expected.usage;
        let delta = cs::MessageDelta::builder()
            .stop_reason(Some(expected.stop_reason))
            .build();
        let usage = cs::MessageDeltaUsage {
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
            cs::StreamEvent::MessageDelta(Box::new(
                cs::MessageDeltaEvent::builder(delta, usage).build(),
            )),
        )?;
        self.emit(
            &mut out,
            cs::StreamEvent::MessageStop(cs::MessageStopEvent::builder().build()),
        )?;
        let actual = self
            .target
            .take()
            .ok_or_else(|| invalid("target consumed"))?
            .finish()?
            .value;
        if actual != expected {
            return Err(invalid(
                "native Claude output differs from canonical Responses conversion",
            ));
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report: self.report,
        })
    }
}
