use super::choice::ChoiceAccum;
use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        openai::chat::{response as r, stream as s},
    },
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Copy)]
pub struct ChatStreamLimits {
    pub max_events: usize,
    pub max_bytes: usize,
    pub max_choices: usize,
    pub max_tool_calls: usize,
}
impl Default for ChatStreamLimits {
    fn default() -> Self {
        Self {
            max_events: 100_000,
            max_bytes: 16 * 1024 * 1024,
            max_choices: 128,
            max_tool_calls: 100_000,
        }
    }
}
#[derive(Debug, Default, Clone)]
pub struct ChatStreamContext {
    pub created: Option<i64>,
    pub model: Option<String>,
    pub response_id: Option<String>,
}
/// Native Chat collector. The SSE decoder must call `push_done` for `[DONE]`;
/// EOF after a finish chunk without the framing terminator remains incomplete.
pub struct ChatStreamCollector {
    context: ChatStreamContext,
    choices: BTreeMap<i64, ChoiceAccum>,
    usage: Option<s::ChunkUsage>,
    service_tier: Option<Option<r::ResponseServiceTier>>,
    fingerprint: Option<String>,
    moderation: Option<Option<r::ModerationResponse>>,
    done: bool,
    failed: bool,
    flow: IdentityFlow,
    target: TargetIdPolicy,
    limits: ChatStreamLimits,
    events: usize,
    bytes: usize,
    tools: usize,
    report: Report,
}
impl ChatStreamCollector {
    pub fn new(flow: IdentityFlow, target: TargetIdPolicy) -> Self {
        Self::with_limits(flow, target, Default::default())
    }
    pub fn with_limits(
        flow: IdentityFlow,
        target: TargetIdPolicy,
        limits: ChatStreamLimits,
    ) -> Self {
        Self {
            context: Default::default(),
            choices: BTreeMap::new(),
            usage: None,
            service_tier: None,
            fingerprint: None,
            moderation: None,
            done: false,
            failed: false,
            flow,
            target,
            limits,
            events: 0,
            bytes: 0,
            tools: 0,
            report: Default::default(),
        }
    }
    pub fn push(&mut self, chunk: s::ChatCompletionChunk) -> Result<(), TransformError> {
        if self.failed || self.done {
            self.failed = true;
            return Err(TransformError::invalid_result(
                "stream",
                "event after terminal or failed collector",
            ));
        }
        let result = self.push_declared(chunk.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_declared(&mut self, chunk: s::ChatCompletionChunk) -> Result<(), TransformError> {
        if self.target.dialect != crate::Dialect::OpenAiChat {
            return Err(TransformError::shape(
                "identity.policy",
                "native Chat collection requires OpenAiChat target policy",
            ));
        }
        self.events = self
            .events
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_events)
            .ok_or_else(|| super::limit("events"))?;
        let remaining = self
            .limits
            .max_bytes
            .checked_sub(self.bytes)
            .ok_or_else(|| super::limit("bytes"))?;
        self.bytes += super::encoded(&chunk, remaining)?;
        if chunk.created < 0 {
            return Err(TransformError::invalid_result(
                "created",
                "negative creation time",
            ));
        }
        if self.context.created.is_some_and(|v| v != chunk.created) {
            return Err(TransformError::invalid_result(
                "created",
                "creation time changed",
            ));
        }
        self.context.created = Some(chunk.created);
        identity(&mut self.context.response_id, chunk.id, "id")?;
        identity(&mut self.context.model, chunk.model, "model")?;
        if let Some(value) = chunk.service_tier {
            if self
                .service_tier
                .flatten()
                .zip(value)
                .is_some_and(|(a, b)| a != b)
            {
                return Err(TransformError::invalid_result(
                    "service_tier",
                    "tier changed during response",
                ));
            }
            if value.is_some() || self.service_tier.is_none() {
                self.service_tier = Some(value);
            }
        }
        if let Some(Some(value)) = chunk.system_fingerprint {
            identity(&mut self.fingerprint, value, "system_fingerprint")?;
        }
        if let Some(value) = chunk.moderation {
            self.moderation = Some(value);
        }
        if chunk.obfuscation.flatten().is_some() {
            self.report
                .omitted("obfuscation", "stream padding is not buffered content");
        }
        if let Some(Some(usage)) = chunk.usage {
            super::usage::validate(&usage)?;
            if self.usage.as_ref().is_some_and(|v| v != &usage) {
                return Err(TransformError::invalid_result(
                    "usage",
                    "conflicting final usage",
                ));
            }
            self.usage = Some(usage);
        }
        let mut indexes = BTreeSet::new();
        for choice in chunk.choices {
            if choice.index < 0 || !indexes.insert(choice.index) {
                return Err(TransformError::invalid_result(
                    "choice.index",
                    "negative or duplicate choice index",
                ));
            }
            if !self.choices.contains_key(&choice.index)
                && self.choices.len() >= self.limits.max_choices
            {
                return Err(super::limit("choices"));
            }
            let state = self.choices.entry(choice.index).or_default();
            let before = state.tool_count();
            state.push(choice)?;
            self.tools = self
                .tools
                .checked_add(state.tool_count() - before)
                .filter(|v| *v <= self.limits.max_tool_calls)
                .ok_or_else(|| super::limit("tool_calls"))?;
        }
        Ok(())
    }
    pub fn push_done(&mut self) -> Result<(), TransformError> {
        if self.failed || self.done {
            self.failed = true;
            return Err(TransformError::invalid_result(
                "stream",
                "duplicate DONE or failed collector",
            ));
        }
        self.done = true;
        if self.choices.is_empty() || self.choices.values().any(|c| !c.is_finished()) {
            self.failed = true;
            return Err(TransformError::invalid_result(
                "stream",
                "DONE before every choice finished",
            ));
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
        if self.failed || !self.done {
            return Err(TransformError::invalid_result(
                "stream",
                "premature EOF or previous stream error",
            ));
        }
        let id = self
            .context
            .response_id
            .ok_or_else(|| TransformError::missing_metadata("id"))?;
        let model = self
            .context
            .model
            .ok_or_else(|| TransformError::missing_metadata("model"))?;
        let created = self
            .context
            .created
            .ok_or_else(|| TransformError::missing_metadata("created"))?;
        let mut choices = Vec::new();
        let mut tool_index = 0;
        for (expected, (index, state)) in self.choices.into_iter().enumerate() {
            if index != expected as i64 {
                return Err(TransformError::invalid_result(
                    "choice.index",
                    "non-contiguous choices",
                ));
            }
            choices.push(state.finish(index, &mut tool_index, &mut self.flow, &self.target)?);
        }
        let mut out = r::GenerateContentResponseBody::builder(
            id,
            choices,
            created,
            model,
            r::CompletionObject::ChatCompletion,
        )
        .build();
        out.usage = self
            .usage
            .map(|u| super::usage::collect(u, &mut self.report));
        out.service_tier = self.service_tier;
        out.system_fingerprint = self.fingerprint;
        out.moderation = self.moderation;
        Ok(Converted {
            value: out,
            report: self.report,
        })
    }
}
fn identity(old: &mut Option<String>, new: String, field: &str) -> Result<(), TransformError> {
    if new.is_empty() {
        return Ok(());
    }
    if old.as_ref().is_some_and(|v| v != &new) {
        return Err(TransformError::invalid_result(
            field,
            "stream identity changed",
        ));
    }
    *old = Some(new);
    Ok(())
}
