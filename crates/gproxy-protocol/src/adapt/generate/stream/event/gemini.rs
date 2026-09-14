use super::*;
use crate::{
    transform::generate::stream::gemini::{GeminiStreamCollector, GeminiStreamLimits},
    wire::gemini as g,
};
impl sealed::Event for g::GenerateContentResponseBody {}
impl NativeEvent for g::GenerateContentResponseBody {
    type Full = Self;
    type Collector = GeminiStreamCollector;
    const DIALECT: Dialect = Dialect::Gemini;
    fn collector(_: IdentityFlow, _: TargetIdPolicy, limits: EventLimits) -> Self::Collector {
        GeminiStreamCollector::new(GeminiStreamLimits {
            max_events: limits.max_events,
            max_bytes: limits.max_bytes,
            max_parts: limits.max_parts,
            max_candidates: limits.max_choices,
        })
    }
    fn collect(collector: &mut Self::Collector, event: Self) -> Result<(), TransformError> {
        collector.push(event)
    }
    fn collected(
        collector: Self::Collector,
    ) -> Result<Converted<Collected<Self::Full>>, TransformError> {
        let value = collector.finish()?;
        Ok(Converted {
            value: Collected::native(value.value),
            report: value.report,
        })
    }
    fn event_name(&self) -> Option<&'static str> {
        None
    }
    fn is_terminal(&self) -> bool {
        self.candidates
            .iter()
            .flatten()
            .any(|v| v.finish_reason.is_some())
    }
    fn tool_declarations(
        &self,
        _complete_tool_names: bool,
    ) -> Vec<(String, ToolCallKind, Option<String>)> {
        self.candidates
            .iter()
            .flatten()
            .flat_map(|c| c.content.iter())
            .flat_map(|c| c.parts.iter().flatten())
            .filter_map(|p| p.function_call.as_ref())
            .filter_map(|f| {
                f.id.as_ref()
                    .map(|id| (id.clone(), ToolCallKind::Function, Some(f.name.clone())))
            })
            .collect()
    }
}
