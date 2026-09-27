use crate::{
    transform::{
        TransformError,
        identity::{IdentityFlow, IdentityRole, IdentityTarget, SourceIdentity, TargetIdPolicy},
    },
    wire::{gemini as g, openai::responses::input as r},
};

pub(crate) fn id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    role: IdentityRole,
    to: IdentityRole,
    source: Option<String>,
    index: u64,
) -> Result<String, TransformError> {
    flow.resolve_as(
        role,
        to,
        SourceIdentity::new(crate::Dialect::Gemini, source, index),
        policy,
    )
    .map(|v| v.emitted_id)
    .map_err(|e| TransformError::shape("identity", e.to_string()))
}

/// The Gemini upstream a Responses request is prepared for, when the caller
/// knows it. Nothing native is restored from it: a signature comes back in the
/// client's own reasoning items (see `signature`).
#[derive(Debug, Default, Clone)]
pub struct GeminiReplayContext {
    pub target: Option<IdentityTarget>,
}

/// The Gemini part a Responses function call stands for, built from what the
/// client sent. A signature is added by the caller from its carrier.
pub(crate) fn function(call: r::FunctionCall) -> Result<g::Part, TransformError> {
    if call.namespace.is_some()
        || call
            .caller
            .flatten()
            .is_some_and(|v| matches!(v, r::Caller::Program(_)))
    {
        return Err(TransformError::unsupported(
            "function.scope",
            "Gemini native tool caller requires adapter",
        ));
    }
    let args: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&call.arguments)
        .map_err(|e| TransformError::shape("function.arguments", e.to_string()))?;
    Ok(g::Part::builder()
        .function_call(
            g::FunctionCall::builder(call.name)
                .id(call.call_id)
                .args(args)
                .build(),
        )
        .build())
}

/// The text a reasoning item shows: its content, or its summary without one.
pub(crate) fn reasoning_text(value: &r::ReasoningItem) -> String {
    match &value.content {
        Some(content) => content.iter().map(|v| v.text.as_str()).collect(),
        None => value.summary.iter().map(|v| v.text.as_str()).collect(),
    }
}
