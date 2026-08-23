//! Provider-independent usage. Moved down from the core: extraction happens
//! in channels, so the type lives at the contract layer.

use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Usage for one exchange. First-class token fields stay deliberately few;
/// everything else is dimensional — a new measure is an entry in `metrics`
/// priced by a data-driven rate rule, not a new column (a first-class
/// column cost v2 34 files).
#[derive(Debug, Clone, Default)]
pub struct NormalizedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    /// Quantities: `"audio_seconds"`, `"video_seconds"`, `"image_output"`...
    pub metrics: BTreeMap<String, Decimal>,
    /// Qualifiers that select pricing variants: `"resolution"`, `"tier"`...
    pub dimensions: BTreeMap<String, String>,
}
