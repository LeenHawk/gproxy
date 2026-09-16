use super::*;
use crate::adapt::generate::fanout::output::Client;
use crate::wire::{
    gemini as g,
    openai::chat::{self as h, stream as hs},
};

/// Sealed by NativeEvent. Rewrites only the aggregate envelope of concrete client events.
pub trait FanoutEvent: NativeEvent {
    fn project(
        &mut self,
        index: usize,
        id: &str,
        created: &mut Option<i64>,
    ) -> Result<(), TransformError>;
    fn aggregate(
        values: Vec<Self::Full>,
        id: String,
        report: &mut Report,
    ) -> Result<Self::Full, TransformError>;
    fn tail(value: &Self::Full, usage: bool) -> Result<Option<Self>, TransformError>;
    fn equivalent(a: &Self::Full, b: &Self::Full) -> bool;
    fn visible(&self) -> bool;
}

impl FanoutEvent for hs::ChatCompletionChunk {
    fn project(
        &mut self,
        index: usize,
        id: &str,
        created: &mut Option<i64>,
    ) -> Result<(), TransformError> {
        self.id = id.into();
        self.created = *created.get_or_insert(self.created);
        self.usage = None;
        self.service_tier = None;
        self.system_fingerprint = None;
        if self.choices.len() > 1 || self.choices.iter().any(|c| c.index != 0) {
            return Err(invalid("child Chat choice is not singleton"));
        }
        for c in &mut self.choices {
            c.index = i64::try_from(index).map_err(|_| limit("choice index overflow"))?;
        }
        Ok(())
    }
    fn aggregate(
        values: Vec<Self::Full>,
        id: String,
        report: &mut Report,
    ) -> Result<Self::Full, TransformError> {
        let mut value = <h::GenerateContentResponseBody as Client>::aggregate(values, id, report)?;
        if value.service_tier.is_some()
            || value.system_fingerprint.is_some()
            || value.moderation.is_some()
        {
            report.omitted(
                "service_tier,system_fingerprint,moderation",
                "aggregate metadata cannot be established before incremental exposure",
            );
        }
        value.service_tier = None;
        value.system_fingerprint = None;
        value.moderation = None;
        Ok(value)
    }
    fn tail(value: &Self::Full, usage: bool) -> Result<Option<Self>, TransformError> {
        if !usage || value.usage.is_none() {
            return Ok(None);
        }
        let mut v = Self::builder(
            value.id.clone(),
            Vec::new(),
            value.created,
            value.model.clone(),
            hs::ChunkObject::ChatCompletionChunk,
        )
        .build();
        v.usage = Some(
            value
                .usage
                .clone()
                .map(crate::transform::generate::stream::chat::usage::synthesize)
                .transpose()?,
        );
        Ok(Some(v))
    }
    fn equivalent(a: &Self::Full, b: &Self::Full) -> bool {
        a == b
    }
    fn visible(&self) -> bool {
        !self.choices.is_empty()
    }
}

impl FanoutEvent for g::GenerateContentResponseBody {
    fn project(
        &mut self,
        index: usize,
        id: &str,
        _: &mut Option<i64>,
    ) -> Result<(), TransformError> {
        self.response_id = Some(id.into());
        self.usage_metadata = None;
        self.model_version = None;
        self.model_status = None;
        self.prompt_feedback = None;
        if self
            .candidates
            .as_ref()
            .is_some_and(|v| v.len() > 1 || v.iter().any(|c| c.index.is_some_and(|i| i != 0)))
        {
            return Err(invalid("child Gemini candidate is not singleton"));
        }
        for c in self.candidates.iter_mut().flatten() {
            c.index = Some(i64::try_from(index).map_err(|_| limit("candidate index overflow"))?);
        }
        Ok(())
    }
    fn aggregate(
        values: Vec<Self::Full>,
        id: String,
        report: &mut Report,
    ) -> Result<Self::Full, TransformError> {
        let mut value = <Self as Client>::aggregate(values, id, report)?;
        if value.model_version.is_some()
            || value.model_status.is_some()
            || value.prompt_feedback.is_some()
        {
            report.omitted(
                "modelVersion,modelStatus,promptFeedback",
                "aggregate metadata cannot be established before incremental exposure",
            );
        }
        value.model_version = None;
        value.model_status = None;
        value.prompt_feedback = None;
        Ok(value)
    }
    fn tail(value: &Self::Full, _: bool) -> Result<Option<Self>, TransformError> {
        let mut v = Self::builder().build();
        v.response_id = value.response_id.clone();
        v.usage_metadata = value.usage_metadata.clone();
        Ok(Some(v))
    }
    fn equivalent(a: &Self::Full, b: &Self::Full) -> bool {
        a == b
    }
    fn visible(&self) -> bool {
        self.candidates.as_ref().is_some_and(|v| !v.is_empty())
    }
}
