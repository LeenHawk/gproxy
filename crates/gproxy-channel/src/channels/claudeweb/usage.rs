//! What the counts in this channel's Messages output are.
//!
//! claude.ai reports no token counts. The translated Messages output carries
//! v3's character estimates instead — of the request as `input_tokens`, of
//! the answer as `output_tokens` — and the host reads them the standard way.
//! This marks the reading for what it is: estimated, and never complete,
//! exactly as the host marks an estimate of its own.

use super::ClaudeWeb;
use crate::channel::{NormalizedUsage, UsageCompleteness, UsageExtras, UsageSource};

impl UsageExtras for ClaudeWeb {
    fn read(&self, _source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        if *usage == NormalizedUsage::default() {
            return;
        }
        usage.completeness = UsageCompleteness::Partial;
        usage
            .dimensions
            .insert("estimated".to_owned(), "true".to_owned());
    }
}
