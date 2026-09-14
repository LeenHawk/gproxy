pub use super::context::ClaudeToResponsesContext;
use super::{
    common::{
        Budget, StreamEnd, StreamLimits, declared, id, invalid, item_id, measure,
        normalize_arguments,
    },
    context::{annotations, clean_context, clone_response_context},
    response_events::ResponseEvents,
    usage::ClaudeProgress,
};
use crate::{
    transform::generate::stream::claude::{ClaudeStreamCollector, ClaudeStreamLimits},
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, TargetIdPolicy},
    },
    wire::{
        claude::{generate_content as c, stream as cs},
        openai::responses::{input as i, response as r, stream as s},
    },
};
use std::collections::{BTreeMap, BTreeSet};
pub(super) struct Block {
    pub output_index: Option<i64>,
    pub item: Option<r::ResponseOutputItem>,
    pub closed: bool,
}
pub struct ClaudeToResponsesStream {
    source: Option<ClaudeStreamCollector>,
    context: ClaudeToResponsesContext,
    pub(super) events: ResponseEvents,
    pub(super) flow: IdentityFlow,
    pub(super) policy: TargetIdPolicy,
    pub(super) budget: Budget,
    pub(super) limits: StreamLimits,
    pub(super) blocks: BTreeMap<i64, Block>,
    pub(super) output_items: usize,
    pub(super) parts: usize,
    pub(super) tool_count: usize,
    pub(super) argument_bytes: usize,
    pub(super) mcp_calls: BTreeMap<String, i64>,
    pub(super) mcp_results: BTreeSet<String>,
    usage: ClaudeProgress,
    failed: bool,
    stopped: bool,
}
impl ClaudeToResponsesStream {
    pub fn new(
        context: impl Into<ClaudeToResponsesContext>,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(
            context,
            flow,
            limits,
            TargetIdPolicy::new(crate::Dialect::OpenAi),
        )
    }
    pub fn new_with_policy(
        context: impl Into<ClaudeToResponsesContext>,
        flow: IdentityFlow,
        limits: StreamLimits,
        policy: TargetIdPolicy,
    ) -> Result<Self, TransformError> {
        if policy.dialect != crate::Dialect::OpenAi {
            return Err(TransformError::shape(
                "stream.policy",
                "target policy dialect mismatch",
            ));
        }

        let context = clean_context(context.into(), limits)?;
        Ok(Self {
            source: Some(ClaudeStreamCollector::new(ClaudeStreamLimits {
                max_events: limits.max_events,
                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
                max_blocks: limits.max_blocks,
            })),
            context,
            events: ResponseEvents::new(limits),
            flow,
            policy,
            budget: Budget::new(limits),
            limits,
            blocks: BTreeMap::new(),
            output_items: 0,
            parts: 0,
            tool_count: 0,
            argument_bytes: 0,
            mcp_calls: BTreeMap::new(),
            mcp_results: BTreeSet::new(),
            usage: Default::default(),
            failed: false,
            stopped: false,
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        event: cs::StreamEvent,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        if self.failed || self.stopped {
            self.failed = true;
            return Err(invalid("event after message_stop or failure"));
        }
        let result = self.inner(declared(event));
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        event: cs::StreamEvent,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        self.budget.input(&event)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(event.clone())?;
        let mut out = Vec::new();
        match event {
            cs::StreamEvent::MessageStart(v) => {
                self.usage.start(&v.message.usage)?;
                let id = id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::Claude,
                    IdentityRole::Response,
                    IdentityRole::Response,
                    Some(v.message.id),
                    0,
                )?;
                let mut base = clone_response_context(&self.context.response).into_response(
                    id,
                    self.context.response.created_at,
                    v.message.model,
                )?;
                base.status = Some(r::ResponseStatus::InProgress);
                base.usage = Some(None);
                measure(&base, self.limits.max_bytes)?;
                self.events
                    .emit(&mut self.budget, &mut out, |sequence_number| {
                        s::StreamEvent::Created(s::ResponseCreated {
                            sequence_number,
                            response: base,
                            rest: Default::default(),
                        })
                    })?;
            }
            cs::StreamEvent::ContentBlockStart(v) => self.start_block(v, &mut out)?,
            cs::StreamEvent::ContentBlockDelta(v) => match v.delta {
                cs::ContentBlockDelta::Text(vv) => self.payload(v.index, vv.text, &mut out)?,
                cs::ContentBlockDelta::Thinking(vv) => {
                    self.payload(v.index, vv.thinking, &mut out)?
                }
                cs::ContentBlockDelta::InputJson(vv) => {
                    self.payload(v.index, vv.partial_json, &mut out)?
                }
                cs::ContentBlockDelta::Signature(_) | cs::ContentBlockDelta::Citations(_) => {}
                cs::ContentBlockDelta::Compaction(_) => {
                    return Err(TransformError::unsupported(
                        "compaction",
                        "replacement history requires an invocation adapter",
                    ));
                }
            },
            cs::StreamEvent::ContentBlockStop(v) => self.stop_block(v.index, &mut out)?,
            cs::StreamEvent::MessageDelta(v) => self.usage.delta(&v.usage)?,
            cs::StreamEvent::MessageStop(_) => self.stopped = true,
            cs::StreamEvent::Ping(_) => {}
            cs::StreamEvent::Error(v) => {
                return Err(invalid(format!("{:?}: {}", v.error.type_, v.error.message)));
            }
        }
        Ok(Converted {
            value: out,
            report: Report::default(),
        })
    }
    pub fn finish(mut self) -> Result<StreamEnd<s::StreamEvent>, TransformError> {
        if self.failed || !self.stopped || self.blocks.values().any(|v| !v.closed) {
            return Err(invalid("failed stream or premature EOF"));
        }
        let mut source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?
            .value;
        self.usage
            .finalize(&mut source.usage, self.context.response.usage)?;
        let refusal = source.stop_reason == c::StopReason::Refusal;
        let mut citation_report = Report::default();
        let citations = annotations(
            std::mem::take(&mut self.context.annotations),
            &source.content,
            &mut citation_report,
        )?;
        let converted = super::super::claude_to_responses_response(
            source,
            self.context.response,
            &mut self.flow.clone(),
            &self.policy,
        )?;
        let mut expected = converted.value;
        let mut report = converted.report;
        report
            .diagnostics
            .retain(|v| v.field != "content.citations");
        report.diagnostics.extend(citation_report.diagnostics);
        if refusal {
            lower_refusal(&mut expected);
            report.changed("stop_reason","late Claude refusal preserves output text and maps the terminal to incomplete/content_filter");
        }
        for (source_index, block) in &self.blocks {
            let Some(index) = block.output_index else {
                continue;
            };
            let item = expected
                .output
                .get_mut(index as usize)
                .ok_or_else(|| invalid("missing canonical output item"))?;
            if item_id(item) != block.item.as_ref().and_then(item_id) {
                return Err(invalid("canonical item identity differs from emitted item"));
            }
            match (item, block.item.as_ref().unwrap()) {
                (
                    r::ResponseOutputItem::FunctionCall(target),
                    r::ResponseOutputItem::FunctionCall(actual),
                ) => target.arguments = checked_raw(&target.arguments, &actual.arguments)?,
                (
                    r::ResponseOutputItem::McpCall(target),
                    r::ResponseOutputItem::McpCall(actual),
                ) => target.arguments = checked_raw(&target.arguments, &actual.arguments)?,
                (r::ResponseOutputItem::Message(target), _) => {
                    if let Some(facts) = citations.get(&(*source_index as usize))
                        && let Some(i::OutputContent::Text(part)) = target.content.first_mut()
                    {
                        part.annotations = facts.clone();
                    }
                }
                _ => {}
            }
        }
        measure(&expected, self.limits.max_bytes)?;
        let mut out = Vec::new();
        for (index, item) in expected.output.iter().cloned().enumerate() {
            self.events
                .item_done(&mut self.budget, &mut out, index as i64, item)?;
        }
        self.events
            .emit(&mut self.budget, &mut out, |sequence_number| {
                if expected.status == Some(r::ResponseStatus::Incomplete) {
                    s::StreamEvent::Incomplete(s::ResponseIncomplete {
                        sequence_number,
                        response: expected.clone(),
                        rest: Default::default(),
                    })
                } else {
                    s::StreamEvent::Completed(s::ResponseCompleted {
                        sequence_number,
                        response: expected.clone(),
                        rest: Default::default(),
                    })
                }
            })?;
        let mut actual = self.events.finish()?;
        normalize_arguments(&mut actual)?;
        normalize_arguments(&mut expected)?;
        if actual != expected {
            return Err(invalid(
                "native Responses output differs from canonical source conversion",
            ));
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report,
        })
    }
}
fn lower_refusal(response: &mut r::GenerateContentResponseBody) {
    for item in &mut response.output {
        if let r::ResponseOutputItem::Message(message) = item {
            for part in &mut message.content {
                if let i::OutputContent::Refusal(value) = part {
                    *part = i::OutputContent::Text(super::response_events::text(
                        std::mem::take(&mut value.refusal),
                        Vec::new(),
                    ));
                }
            }
        }
    }
    response.status = Some(r::ResponseStatus::Incomplete);
    response.incomplete_details = Some(r::ResponseIncompleteDetails {
        reason: Some(r::ResponseIncompleteReason::ContentFilter),
        rest: Default::default(),
    });
}

fn checked_raw(canonical: &str, actual: &str) -> Result<String, TransformError> {
    let canonical: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(canonical).map_err(|e| invalid(e.to_string()))?;
    let parsed: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(actual).map_err(|e| invalid(e.to_string()))?;
    if canonical != parsed {
        return Err(invalid(
            "streamed arguments differ from actual native tool input",
        ));
    }
    Ok(actual.to_owned())
}
