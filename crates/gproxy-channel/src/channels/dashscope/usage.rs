//! The image envelope's own counters.

use super::DashScope;
use crate::channel::{NormalizedUsage, UsageCompleteness, UsageExtras, UsageSource};
use serde_json::Value;

impl UsageExtras for DashScope {
    /// The shaped image reply keeps DashScope's own usage object under its
    /// own name (`image.rs`), counting whole images rather than OpenAI's token
    /// details. Chat replies are plain Chat Completions and add nothing.
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        let root = source.root;
        let Some(native) = root.get("dashscope_usage") else {
            return;
        };
        let count = |name: &str| native.get(name).and_then(Value::as_u64);
        usage.tokens.input_tokens = count("input_tokens");
        usage.tokens.output_tokens = count("output_tokens");
        if let Some(images) = count("image_count") {
            usage.metrics.insert("image_outputs".into(), images.into());
        }
        if let Some(size) = native
            .get("size")
            .or_else(|| root.get("size"))
            .and_then(Value::as_str)
        {
            usage.dimensions.insert("size".into(), size.to_owned());
        }
        usage.completeness = UsageCompleteness::Complete;
    }
}
