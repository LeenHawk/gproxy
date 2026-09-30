//! Concrete native event collection. The associated payloads remain vendor wire types.

mod chat;
mod claude;
mod gemini;
mod responses;
use super::super::{
    ToolCallKind,
    identity_facts::{IdentityFacts, ToolIdentity},
};
use crate::{
    Dialect,
    transform::{
        Converted, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::DeclaredFields,
};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Copy)]
pub struct EventLimits {
    pub max_bytes: usize,
    pub max_pending_bytes: usize,
}

impl EventLimits {}

#[derive(Debug, Clone)]
pub struct Collected<T> {
    pub value: T,
    /// Actual source observations override IDs synthesized only to construct a
    /// complete native Chat DTO. These are never invented upstream identities.
    pub(crate) original_tool_ids: Option<Vec<Option<String>>>,
}

impl<T> Collected<T> {
    fn native(value: T) -> Self {
        Self {
            value,
            original_tool_ids: None,
        }
    }
}

impl<T: IdentityFacts> IdentityFacts for Collected<T> {
    fn dialect(&self) -> Dialect {
        self.value.dialect()
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        let mut tools = self.value.tools();
        if let Some(ids) = &self.original_tool_ids {
            // The DTO's native legacy/modern form remains intact. Only IDs
            // synthesized for DTO completeness are replaced by actual observations.
            for (tool, id) in tools.iter_mut().zip(ids) {
                tool.call_id.clone_from(id);
            }
        }
        tools
    }
}

pub(crate) mod sealed {
    pub trait Event {}
}

/// Sealed over the four actual generation event types.
pub trait NativeEvent:
    sealed::Event + Serialize + DeserializeOwned + DeclaredFields + Clone
{
    type Full: Serialize + Clone + DeclaredFields;
    type Collector;
    fn responses_history(
        _value: &Self::Full,
    ) -> Option<&crate::wire::openai::responses::GenerateContentResponseBody> {
        None
    }
    const DIALECT: Dialect;
    const DONE: bool = false;
    fn collector(
        flow: IdentityFlow,
        policy: TargetIdPolicy,
        limits: EventLimits,
    ) -> Self::Collector;
    fn collect(collector: &mut Self::Collector, event: Self) -> Result<(), TransformError>;
    fn collect_done(_collector: &mut Self::Collector) -> Result<(), TransformError> {
        Err(invalid("unexpected DONE for native dialect"))
    }
    fn collected(
        collector: Self::Collector,
    ) -> Result<Converted<Collected<Self::Full>>, TransformError>;
    fn event_name(&self) -> Option<&'static str>;
    fn is_terminal(&self) -> bool;
    fn is_usage_only(&self) -> bool {
        false
    }
    /// Complete emitted ID, actual call kind, and complete name if already known.
    fn tool_declarations(
        &self,
        complete_tool_names: bool,
    ) -> Vec<(String, ToolCallKind, Option<String>)>;
}

fn invalid(message: impl Into<String>) -> TransformError {
    TransformError::invalid_result("generation.stream.event", message)
}
