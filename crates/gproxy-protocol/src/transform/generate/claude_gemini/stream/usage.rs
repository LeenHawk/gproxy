use super::super::ClaudeGeminiUsageFacts as Facts;
use super::common::invalid;
use crate::{
    transform::{Report, TransformError},
    wire::{
        claude::{generate_content as c, stream as s},
        gemini as g,
    },
};
fn count(n: i64) -> Result<i64, TransformError> {
    if n < 0 {
        Err(invalid("usage", "negative token count"))
    } else {
        Ok(n)
    }
}
fn add(a: i64, b: i64) -> Result<i64, TransformError> {
    count(a)?
        .checked_add(count(b)?)
        .ok_or_else(|| invalid("usage", "token count overflow"))
}
fn agree(a: Option<i64>, b: Option<i64>, field: &str) -> Result<Option<i64>, TransformError> {
    if a.zip(b).is_some_and(|(a, b)| a != b) {
        return Err(invalid(field, "factual counters disagree"));
    }
    a.or(b).map(count).transpose()
}
pub(super) fn validate_facts(f: Facts) -> Result<(), TransformError> {
    for n in [
        f.cache_creation_input_tokens,
        f.cache_read_input_tokens,
        f.thinking_tokens,
    ]
    .into_iter()
    .flatten()
    {
        count(n)?;
    }
    Ok(())
}
fn written(usage: &c::Usage) -> Result<Option<i64>, TransformError> {
    let detail = usage
        .cache_creation
        .as_ref()
        .and_then(Option::as_ref)
        .map(|v| add(v.ephemeral_1h_input_tokens, v.ephemeral_5m_input_tokens))
        .transpose()?;
    agree(
        usage.cache_creation_input_tokens.flatten(),
        detail,
        "usage.cache_creation",
    )
}
pub(super) fn prepare_initial(usage: &mut c::Usage, facts: Facts) -> Result<(), TransformError> {
    count(usage.input_tokens)?;
    count(usage.output_tokens)?;
    let write = agree(
        written(usage)?,
        facts.cache_creation_input_tokens,
        "usage.cache_creation",
    )?;
    let read = agree(
        usage.cache_read_input_tokens.flatten(),
        facts.cache_read_input_tokens,
        "usage.cache_read",
    )?;
    if write.is_some() {
        usage.cache_creation_input_tokens = write.map(Some);
    }
    if read.is_some() {
        usage.cache_read_input_tokens = read.map(Some);
    }
    if let Some(v) = usage
        .output_tokens_details
        .as_ref()
        .and_then(Option::as_ref)
        && (count(v.thinking_tokens)? > usage.output_tokens
            || facts.thinking_tokens.is_some_and(|n| n < v.thinking_tokens))
    {
        return Err(invalid(
            "usage.thinking",
            "initial thinking contradicts output/final facts",
        ));
    }
    Ok(())
}
#[derive(Default)]
pub(super) struct ClaudeUsageProgress {
    last_output: i64,
    highest_thinking: Option<i64>,
    thinking_at_output: Option<i64>,
}
impl ClaudeUsageProgress {
    pub fn start(&mut self, usage: &c::Usage) -> Result<(), TransformError> {
        self.observe(
            usage.output_tokens,
            usage
                .output_tokens_details
                .as_ref()
                .and_then(Option::as_ref)
                .map(|v| v.thinking_tokens),
            usage.output_tokens_details.is_some(),
        )
    }
    pub fn delta(&mut self, usage: &s::MessageDeltaUsage) -> Result<(), TransformError> {
        self.observe(
            usage.output_tokens,
            usage
                .output_tokens_details
                .as_ref()
                .and_then(Option::as_ref)
                .map(|v| v.thinking_tokens),
            usage.output_tokens_details.is_some(),
        )
    }
    fn observe(
        &mut self,
        output: i64,
        thinking: Option<i64>,
        present: bool,
    ) -> Result<(), TransformError> {
        if count(output)? < self.last_output {
            return Err(invalid(
                "usage.output_tokens",
                "cumulative output decreased",
            ));
        }
        self.last_output = output;
        if let Some(n) = thinking {
            if count(n)? > output || self.highest_thinking.is_some_and(|old| n < old) {
                return Err(invalid(
                    "usage.thinking_tokens",
                    "cumulative thinking decreased or exceeds output",
                ));
            }
            self.highest_thinking = Some(n);
            self.thinking_at_output = Some(output);
        } else if present {
            self.thinking_at_output = None;
        }
        Ok(())
    }
    pub fn finalize(&self, usage: &mut c::Usage, facts: Facts) -> Result<(), TransformError> {
        if facts
            .thinking_tokens
            .zip(self.highest_thinking)
            .is_some_and(|(n, old)| n < old)
        {
            return Err(invalid(
                "usage.thinking_tokens",
                "final fact is below observed thinking",
            ));
        }
        if self.thinking_at_output != Some(usage.output_tokens) {
            usage.output_tokens_details = None;
        }
        Ok(())
    }
}
pub(super) struct GeminiUsageProgress {
    usage: g::UsageMetadata,
    candidate_total: Option<i64>,
    thinking_total: Option<i64>,
}
impl Default for GeminiUsageProgress {
    fn default() -> Self {
        Self {
            usage: g::UsageMetadata::builder().build(),
            candidate_total: None,
            thinking_total: None,
        }
    }
}
impl GeminiUsageProgress {
    pub fn observe(&mut self, new: &g::UsageMetadata) {
        macro_rules! field {
            ($field:ident) => {
                if new.$field.is_some() {
                    self.usage.$field = new.$field;
                }
            };
        }
        field!(prompt_token_count);
        field!(cached_content_token_count);
        field!(candidates_token_count);
        field!(tool_use_prompt_token_count);
        field!(thoughts_token_count);
        field!(total_token_count);
        if new.candidates_token_count.is_some() {
            self.candidate_total = self.usage.total_token_count;
        }
        if new.thoughts_token_count.is_some() {
            self.thinking_total = self.usage.total_token_count;
        }
    }
    /// Components from an earlier cumulative total are lower bounds, not a
    /// contradictory final split. Resolve only from actual current components,
    /// exact totals or an explicitly supplied final thinking observation.
    pub fn effective(
        &self,
        usage: &mut g::UsageMetadata,
        final_thinking: Option<i64>,
    ) -> Result<bool, TransformError> {
        let (Some(total), Some(prompt)) = (usage.total_token_count, usage.prompt_token_count)
        else {
            return Ok(false);
        };
        let output = count(total)?
            .checked_sub(add(prompt, usage.tool_use_prompt_token_count.unwrap_or(0))?)
            .filter(|v| *v >= 0)
            .ok_or_else(|| invalid("usage", "total below actual prompt"))?;
        let old_c = usage.candidates_token_count;
        let old_t = usage.thoughts_token_count;
        let mut candidates = old_c.filter(|_| self.candidate_total == Some(total));
        let native_thinking = old_t.filter(|_| self.thinking_total == Some(total));
        let mut thinking = agree(native_thinking, final_thinking, "usage.thinking_tokens")?;
        if candidates.is_none()
            && thinking.is_none()
            && let (Some(c), Some(t)) = (old_c, old_t)
            && add(c, t)? == output
        {
            candidates = Some(c);
            thinking = Some(t);
        }
        match (candidates, thinking) {
            (Some(c), Some(t)) if add(c, t)? != output => {
                return Err(invalid(
                    "usage",
                    "current output components disagree with total",
                ));
            }
            (Some(c), None) => {
                thinking = Some(
                    output
                        .checked_sub(count(c)?)
                        .filter(|v| *v >= 0)
                        .ok_or_else(|| invalid("usage", "candidates exceed output"))?,
                )
            }
            (None, Some(t)) => {
                candidates = Some(
                    output
                        .checked_sub(count(t)?)
                        .filter(|v| *v >= 0)
                        .ok_or_else(|| invalid("usage", "thinking exceeds output"))?,
                )
            }
            _ => {}
        }
        if old_c.zip(candidates).is_some_and(|(old, new)| new < old)
            || old_t.zip(thinking).is_some_and(|(old, new)| new < old)
        {
            return Err(invalid(
                "usage",
                "resolved output component decreased from observed prefix",
            ));
        }
        usage.candidates_token_count = candidates;
        // Keep native absence explicit; the pair helper derives the same
        // thinking value from current candidates/total and validates facts.
        usage.thoughts_token_count = native_thinking;
        Ok(usage.candidates_token_count != old_c || usage.thoughts_token_count != old_t)
    }
    pub fn usage(&self) -> &g::UsageMetadata {
        &self.usage
    }
}
/// Resolve fixed input/cache facts before calculating the Claude uncached input.
/// Thinking facts are final observations, never filled from an initial zero.
pub(super) fn gemini_usage(
    input: g::UsageMetadata,
    mut facts: Facts,
    initial: Option<&c::Usage>,
    report: &mut Report,
) -> Result<(c::Usage, Facts), TransformError> {
    validate_facts(facts)?;
    if let Some(initial) = initial {
        facts.cache_creation_input_tokens = agree(
            facts.cache_creation_input_tokens,
            written(initial)?,
            "usage.cache_creation",
        )?;
        facts.cache_read_input_tokens = agree(
            facts.cache_read_input_tokens,
            initial.cache_read_input_tokens.flatten(),
            "usage.cache_read",
        )?;
        if facts.cache_creation_input_tokens.is_none() {
            // Every term must be factual; an omitted tool-prompt count is not
            // manufactured solely to derive a missing cache-write counter.
            if let (Some(prompt), Some(tool), Some(cached)) = (
                input.prompt_token_count,
                input.tool_use_prompt_token_count,
                input
                    .cached_content_token_count
                    .or(facts.cache_read_input_tokens),
            ) {
                let remaining = add(prompt, tool)?
                    .checked_sub(count(cached)?)
                    .and_then(|v| v.checked_sub(initial.input_tokens))
                    .filter(|v| *v >= 0)
                    .ok_or_else(|| {
                        invalid(
                            "usage.cache_creation",
                            "known input exceeds prompt/cache totals",
                        )
                    })?;
                facts.cache_creation_input_tokens = Some(remaining);
            }
        }
    }
    let mut usage = super::super::usage::to_claude(input, facts, report)?;
    if let Some(initial) = initial {
        if usage.input_tokens != initial.input_tokens || usage.output_tokens < initial.output_tokens
        {
            return Err(invalid(
                "usage.initial",
                "final counters contradict factual initial input/output",
            ));
        }
        if let Some(old) = initial
            .output_tokens_details
            .as_ref()
            .and_then(Option::as_ref)
            && usage
                .output_tokens_details
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|new| new.thinking_tokens < old.thinking_tokens)
        {
            return Err(invalid(
                "usage.thinking",
                "final thinking decreased from initial observation",
            ));
        }
        // These are explicit native target facts supplied by the caller. The
        // completed-source converter has no corresponding Gemini fields.
        usage.cache_creation = initial.cache_creation.clone();
        usage.fallback_credit = initial.fallback_credit.clone();
        usage.inference_geo = initial.inference_geo.clone();
        usage.iterations = initial.iterations.clone();
        usage.server_tool_use = initial.server_tool_use.clone();
        usage.service_tier = initial.service_tier;
        usage.speed = initial.speed;
    }
    Ok((usage, facts))
}

pub(super) fn validate_output_progress(
    initial: &c::Usage,
    final_usage: &c::Usage,
) -> Result<(), TransformError> {
    if final_usage.output_tokens < initial.output_tokens
        || initial
            .output_tokens_details
            .as_ref()
            .and_then(Option::as_ref)
            .zip(
                final_usage
                    .output_tokens_details
                    .as_ref()
                    .and_then(Option::as_ref),
            )
            .is_some_and(|(old, new)| new.thinking_tokens < old.thinking_tokens)
    {
        return Err(invalid(
            "usage",
            "output or thinking decreased from actual start snapshot",
        ));
    }
    Ok(())
}
