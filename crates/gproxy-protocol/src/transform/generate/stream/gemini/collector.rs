use crate::{
    codec::{self, CodecErrorKind, CodecLimits},
    transform::{Converted, TransformError, TransformErrorKind},
    wire::{DeclaredFields, gemini as g},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy)]
pub struct GeminiStreamLimits {
    pub max_events: usize,
    pub max_bytes: usize,
    pub max_candidates: usize,
    pub max_parts: usize,
}

impl Default for GeminiStreamLimits {
    fn default() -> Self {
        Self {
            max_events: usize::MAX,
            max_bytes: usize::MAX,
            max_candidates: usize::MAX,
            max_parts: usize::MAX,
        }
    }
}

/// Collects native chunks without retaining a second copy of the event log.
/// Content is incremental; usage and non-content metadata are native cumulative
/// snapshots. A failed push poisons the collector so partial state cannot pass.
pub struct GeminiStreamCollector {
    limits: GeminiStreamLimits,
    events: usize,
    bytes: usize,
    parts: usize,
    failed: bool,
    body: g::GenerateContentResponseBody,
    candidates: BTreeMap<i64, g::Candidate>,
    seen_candidates: bool,
}

impl GeminiStreamCollector {
    pub fn new(limits: GeminiStreamLimits) -> Self {
        Self {
            limits,
            events: 0,
            bytes: 0,
            parts: 0,
            failed: false,
            body: g::GenerateContentResponseBody::builder().build(),
            candidates: BTreeMap::new(),
            seen_candidates: false,
        }
    }
    pub fn push(&mut self, chunk: g::GenerateContentResponseBody) -> Result<(), TransformError> {
        if self.failed {
            return Err(TransformError::invalid_result(
                "stream",
                "collector already failed",
            ));
        }
        let result = self.push_declared(chunk.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_declared(
        &mut self,
        chunk: g::GenerateContentResponseBody,
    ) -> Result<(), TransformError> {
        add_limit(&mut self.events, 1, self.limits.max_events, "events")?;
        // Serialization measures the already rebuilt declared event. It is
        // never used to discover fields or convert between protocols.
        let remaining = self.limits.max_bytes.saturating_sub(self.bytes) as u64;
        let encoded = codec::encode_json(
            &chunk,
            CodecLimits {
                max_buffer_bytes: remaining,
                max_value_bytes: remaining,
                max_body_bytes: remaining,
                max_line_bytes: remaining,
                max_part_bytes: remaining,
                max_parts: self.limits.max_parts,
            },
        )
        .map_err(|error| {
            if error.kind() == CodecErrorKind::Limit {
                limit("bytes")
            } else {
                TransformError::invalid_result("stream.event", error.to_string())
            }
        })?;
        add_limit(
            &mut self.bytes,
            encoded.len(),
            self.limits.max_bytes,
            "bytes",
        )?;
        merge_identity(&mut self.body.response_id, chunk.response_id, "response_id")?;
        merge_identity(
            &mut self.body.model_version,
            chunk.model_version,
            "model_version",
        )?;
        if let Some(usage) = chunk.usage_metadata {
            let previous = self
                .body
                .usage_metadata
                .get_or_insert_with(|| g::UsageMetadata::builder().build());
            super::merge::usage(previous, usage)?;
        }
        if chunk.prompt_feedback.is_some() {
            self.body.prompt_feedback = chunk.prompt_feedback;
        }
        if chunk.model_status.is_some() {
            self.body.model_status = chunk.model_status;
        }
        if let Some(candidates) = chunk.candidates {
            self.seen_candidates = true;
            let multiple = candidates.len() > 1;
            let mut in_chunk = BTreeSet::new();
            for mut candidate in candidates {
                let index = match candidate.index {
                    Some(n) if n >= 0 => n,
                    Some(_) => {
                        return Err(TransformError::invalid_result(
                            "candidate.index",
                            "negative index",
                        ));
                    }
                    None if !multiple => 0,
                    None => {
                        return Err(TransformError::missing_metadata(
                            "multi-candidate stream index",
                        ));
                    }
                };
                if !in_chunk.insert(index) {
                    return Err(TransformError::invalid_result(
                        "candidate.index",
                        "duplicate index within event",
                    ));
                }
                if !self.candidates.contains_key(&index)
                    && self.candidates.len() >= self.limits.max_candidates
                {
                    return Err(limit("candidates"));
                }
                candidate.index = Some(index);
                let previous = self
                    .candidates
                    .entry(index)
                    .or_insert_with(|| g::Candidate::builder().index(index).build());
                if terminal(previous.finish_reason)
                    && candidate
                        .content
                        .as_ref()
                        .and_then(|c| c.parts.as_ref())
                        .is_some_and(|p| !p.is_empty())
                {
                    return Err(TransformError::invalid_result(
                        "candidate.content",
                        "content after candidate terminal",
                    ));
                }
                if let Some(content) = candidate.content.take() {
                    add_limit(
                        &mut self.parts,
                        content.parts.as_ref().map_or(0, Vec::len),
                        self.limits.max_parts,
                        "parts",
                    )?;
                    let old = previous
                        .content
                        .get_or_insert_with(|| g::Content::builder().build());
                    merge_identity(&mut old.role, content.role, "candidate.role")?;
                    if let Some(parts) = content.parts {
                        old.parts.get_or_insert_with(Vec::new).extend(parts);
                    }
                }
                super::merge::candidate(previous, candidate)?;
            }
        }
        Ok(())
    }
    /// EOF is successful only once every observed candidate has a terminal
    /// reason, or the prompt was explicitly blocked before creating candidates.
    pub fn finish(mut self) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        if self.failed {
            return Err(TransformError::invalid_result(
                "stream",
                "collector failed before EOF",
            ));
        }
        let blocked = self
            .body
            .prompt_feedback
            .as_ref()
            .and_then(|v| v.block_reason)
            .is_some_and(|r| r != g::BlockReason::Unspecified);
        if self.events == 0
            || (self.candidates.is_empty() && !blocked)
            || self.candidates.values().any(|v| !terminal(v.finish_reason))
        {
            return Err(TransformError::invalid_result(
                "stream",
                "premature EOF without all candidate terminals",
            ));
        }
        if self.seen_candidates {
            self.body.candidates = Some(self.candidates.into_values().collect());
        }
        Ok(Converted {
            value: self.body,
            report: Default::default(),
        })
    }
}

pub(super) fn terminal(reason: Option<g::FinishReason>) -> bool {
    reason.is_some_and(|r| r != g::FinishReason::Unspecified)
}

pub(super) fn limit(field: &str) -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        format!("stream.{field}"),
        "Gemini stream limit exceeded",
    )
}

fn add_limit(total: &mut usize, n: usize, max: usize, field: &str) -> Result<(), TransformError> {
    *total = total
        .checked_add(n)
        .filter(|n| *n <= max)
        .ok_or_else(|| limit(field))?;
    Ok(())
}

fn merge_identity(
    old: &mut Option<String>,
    new: Option<String>,
    field: &str,
) -> Result<(), TransformError> {
    if let Some(value) = new {
        if value.is_empty() || old.as_ref().is_some_and(|v| v != &value) {
            return Err(TransformError::invalid_result(
                field,
                "empty or conflicting stream identity",
            ));
        }
        *old = Some(value);
    }
    Ok(())
}
