use super::{GeminiStreamCollector, GeminiStreamLimits};
use crate::{
    transform::{Converted, TransformError},
    wire::{DeclaredFields, gemini as g},
};
/// Gemini streams contain the same native response body as buffered delivery.
/// A single terminal chunk is a valid synthesized stream. Validation shares the
/// collector's complete lifecycle and memory bounds; no counters are invented.
pub fn synthesize_gemini_stream(
    input: g::GenerateContentResponseBody,
    limits: GeminiStreamLimits,
) -> Result<Converted<Vec<g::GenerateContentResponseBody>>, TransformError> {
    let input = input.into_declared();
    let mut collector = GeminiStreamCollector::new(limits);
    collector.push(input)?;
    let validated = collector.finish()?;
    Ok(Converted {
        value: vec![validated.value],
        report: validated.report,
    })
}
