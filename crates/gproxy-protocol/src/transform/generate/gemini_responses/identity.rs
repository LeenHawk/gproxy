use crate::{
    transform::{
        TransformError,
        identity::{
            IdentityFlow, IdentityRole, IdentityStateRecord, IdentityTarget, OutputItemKind,
            SourceIdentity, TargetIdPolicy,
        },
    },
    wire::{DeclaredFields, gemini as g, openai::responses::input as r},
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
pub struct RestoredGeminiPart {
    pub state: IdentityStateRecord,
    pub part: g::Part,
}
pub struct RestoredGeminiImage {
    pub state: IdentityStateRecord,
    pub part: g::Part,
    pub materialized: g::Blob,
}
#[derive(Default)]
pub struct GeminiReplayContext {
    pub target: Option<IdentityTarget>,
    pub parts: std::collections::BTreeMap<String, RestoredGeminiPart>,
    pub image_files: std::collections::BTreeMap<String, RestoredGeminiImage>,
}
pub(crate) fn reasoning(
    value: r::ReasoningItem,
    model: &str,
    context: &mut GeminiReplayContext,
) -> Result<g::Part, TransformError> {
    let native = context
        .parts
        .remove(&value.id)
        .ok_or_else(|| TransformError::missing_metadata("reasoning native Gemini replay record"))?;
    let part = validate(
        native,
        &value.id,
        model,
        context,
        IdentityRole::OutputItem(OutputItemKind::Reasoning),
    )?;
    let text = value
        .content
        .map(|v| v.into_iter().map(|v| v.text).collect::<Vec<_>>().join(""))
        .unwrap_or_else(|| {
            value
                .summary
                .into_iter()
                .map(|v| v.text)
                .collect::<Vec<_>>()
                .join("")
        });
    if part.thought != Some(true)
        || part.text.as_deref() != Some(text.as_str())
        || value.encrypted_content.flatten().is_some()
    {
        return Err(TransformError::shape(
            "reasoning",
            "foreign/modified reasoning cannot reuse Gemini signature",
        ));
    }
    Ok(part)
}
pub(super) fn validate(
    native: RestoredGeminiPart,
    key: &str,
    model: &str,
    context: &GeminiReplayContext,
    role: IdentityRole,
) -> Result<g::Part, TransformError> {
    let target = context
        .target
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("Gemini replay target"))?;
    if target.dialect != crate::Dialect::Gemini
        || target.model != model
        || target.origin.as_ref().is_none_or(|v| v.trim().is_empty())
    {
        return Err(TransformError::shape(
            "replay.target",
            "Gemini original model/origin required",
        ));
    }
    native
        .state
        .validate_for(target)
        .map_err(|e| TransformError::shape("replay.state", e.to_string()))?;
    let bound = if role == IdentityRole::ToolCall {
        native.state.client_call_id.as_deref()
    } else {
        native.state.client_item_id.as_deref()
    };
    let part = native.part.into_declared();
    if native.state.role != role
        || bound != Some(key)
        || native
            .state
            .opaque_signature
            .as_ref()
            .map(|v| v.value.as_str())
            != part.thought_signature.as_deref()
        || part.thought_signature.is_none()
        || native
            .state
            .opaque_signature
            .as_ref()
            .is_none_or(|signature| {
                signature.field
                    != crate::transform::identity::OpaqueField::GeminiPartThoughtSignature
            })
    {
        return Err(TransformError::shape(
            "replay.signature",
            "signature/field/identity does not match original record",
        ));
    }
    Ok(part)
}
pub(crate) fn function(
    call: r::FunctionCall,
    model: &str,
    context: &mut GeminiReplayContext,
) -> Result<g::Part, TransformError> {
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
    if let Some(native) = context.parts.remove(&call.call_id) {
        let part = validate(
            native,
            &call.call_id,
            model,
            context,
            IdentityRole::ToolCall,
        )?;
        if part
            .function_call
            .as_ref()
            .is_none_or(|v| v.name != call.name || v.args.as_ref() != Some(&args))
        {
            return Err(TransformError::shape(
                "function.replay",
                "changed function cannot reuse Gemini signature",
            ));
        }
        Ok(part)
    } else {
        Ok(g::Part::builder()
            .function_call(
                g::FunctionCall::builder(call.name)
                    .id(call.call_id)
                    .args(args)
                    .build(),
            )
            .build())
    }
}
