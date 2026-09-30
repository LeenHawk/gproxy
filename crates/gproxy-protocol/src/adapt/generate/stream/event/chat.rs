use super::*;
use crate::{
    transform::generate::stream::chat::{ChatStreamCollector, ChatStreamLimits},
    wire::openai::chat::{self as c, stream as s},
};
use std::collections::BTreeMap;

#[derive(Default)]
struct ChoiceIds {
    legacy: bool,
    tools: BTreeMap<i64, Option<String>>,
}

pub struct ChatCollector {
    native: ChatStreamCollector,
    ids: BTreeMap<i64, ChoiceIds>,
}

impl sealed::Event for s::ChatCompletionChunk {}
impl NativeEvent for s::ChatCompletionChunk {
    type Full = c::GenerateContentResponseBody;
    type Collector = ChatCollector;
    const DIALECT: Dialect = Dialect::OpenAiChat;
    const DONE: bool = true;
    fn collector(
        flow: IdentityFlow,
        mut policy: TargetIdPolicy,
        limits: EventLimits,
    ) -> Self::Collector {
        policy.preserve_source_ids = true;
        ChatCollector {
            native: ChatStreamCollector::with_limits(
                flow,
                policy,
                ChatStreamLimits {
                    max_bytes: limits.max_bytes,
                },
            ),
            ids: BTreeMap::new(),
        }
    }
    fn collect(collector: &mut Self::Collector, event: Self) -> Result<(), TransformError> {
        collector.native.push(event.clone())?;
        for choice in event.choices {
            let observed = collector.ids.entry(choice.index).or_default();
            if choice.delta.function_call.flatten().is_some() {
                observed.legacy = true;
            }
            for tool in choice.delta.tool_calls.flatten().into_iter().flatten() {
                let original = observed.tools.entry(tool.index).or_default();
                if let Some(id) = tool.id.flatten() {
                    *original = Some(id);
                }
            }
        }
        Ok(())
    }
    fn collect_done(collector: &mut Self::Collector) -> Result<(), TransformError> {
        collector.native.push_done()
    }
    fn collected(
        collector: Self::Collector,
    ) -> Result<Converted<Collected<Self::Full>>, TransformError> {
        let value = collector.native.finish()?;
        let observed: Vec<_> = collector
            .ids
            .into_values()
            .flat_map(|choice| {
                if choice.legacy {
                    vec![(None, super::super::super::ChatCallForm::LegacyFunction)]
                } else {
                    choice
                        .tools
                        .into_values()
                        .map(|id| (id, super::super::super::ChatCallForm::Modern))
                        .collect()
                }
            })
            .collect();
        let tools = value.value.tools();
        if observed.len() != tools.len()
            || observed
                .iter()
                .zip(&tools)
                .any(|((_, form), tool)| tool.chat_form != Some(*form))
        {
            return Err(invalid(
                "native Chat tool observation cardinality or original wire form changed",
            ));
        }
        let ids = observed.into_iter().map(|(id, _)| id).collect();
        Ok(Converted {
            value: Collected {
                value: value.value,
                original_tool_ids: Some(ids),
            },
            report: value.report,
        })
    }
    fn event_name(&self) -> Option<&'static str> {
        None
    }
    fn is_terminal(&self) -> bool {
        self.choices
            .iter()
            .any(|c| c.finish_reason.is_some_and(|v| v.is_some()))
    }
    fn is_usage_only(&self) -> bool {
        self.choices.is_empty() && self.usage.as_ref().and_then(Option::as_ref).is_some()
    }
    fn tool_declarations(
        &self,
        complete_tool_names: bool,
    ) -> Vec<(String, ToolCallKind, Option<String>)> {
        self.choices
            .iter()
            .flat_map(|c| {
                c.delta
                    .tool_calls
                    .as_ref()
                    .and_then(Option::as_ref)
                    .into_iter()
                    .flatten()
            })
            .filter_map(|tool| {
                tool.id.as_ref().and_then(Option::as_ref).map(|id| {
                    (
                        id.clone(),
                        ToolCallKind::Function,
                        if complete_tool_names {
                            tool.function
                                .as_ref()
                                .and_then(Option::as_ref)
                                .and_then(|v| v.name.as_ref().and_then(Option::as_ref))
                                .filter(|v| !v.is_empty())
                                .cloned()
                        } else {
                            None
                        },
                    )
                })
            })
            .collect()
    }
}
