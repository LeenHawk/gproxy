use super::common::invalid;
use crate::{transform::TransformError, wire::gemini as g};
fn count(n: i64) -> Result<i64, TransformError> {
    Ok(n)
}
fn add(a: i64, b: i64) -> Result<i64, TransformError> {
    count(a)?
        .checked_add(count(b)?)
        .ok_or_else(|| invalid("count overflow"))
}
fn agree(a: Option<i64>, b: Option<i64>, _: &str) -> Result<Option<i64>, TransformError> {
    a.or(b).map(count).transpose()
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
            .ok_or_else(|| invalid("usage: total below actual prompt"))?;
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
        if let (None, Some(thinking)) = (candidates, thinking) {
            candidates = Some(output - thinking);
        }

        usage.candidates_token_count = candidates;
        // Keep native absence explicit; the pair helper derives the same
        // thinking value from current candidates/total and validates facts.
        usage.thoughts_token_count = native_thinking;
        Ok(usage.candidates_token_count != old_c || usage.thoughts_token_count != old_t)
    }
}
