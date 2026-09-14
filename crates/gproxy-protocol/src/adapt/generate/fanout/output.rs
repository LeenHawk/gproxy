use super::super::identity_facts::{IdentityFacts, ToolIdentity};
use super::*;
use crate::{
    Dialect,
    transform::Report,
    wire::{DeclaredFields, gemini as g, openai::chat as h},
};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeSet;
mod chat;
mod gemini;
pub(in crate::adapt::generate) trait Client:
    Serialize + DeserializeOwned + Clone + DeclaredFields + IdentityFacts
{
    fn set_calls(&mut self, ids: Vec<Option<String>>);
    fn signed_calls(&self) -> Vec<bool> {
        vec![false; self.tools().len()]
    }
    fn aggregate(
        values: Vec<Self>,
        id: String,
        report: &mut Report,
    ) -> Result<Self, TransformError>;
}
impl Client for h::GenerateContentResponseBody {
    fn set_calls(&mut self, ids: Vec<Option<String>>) {
        for (call, id) in self
            .choices
            .iter_mut()
            .flat_map(|c| c.message.tool_calls.iter_mut().flatten())
            .zip(ids)
        {
            match call {
                h::MessageToolCall::Function(c) => c.id = id.expect("Chat calls always have an ID"),
                h::MessageToolCall::Custom(c) => c.id = id.expect("Chat calls always have an ID"),
            }
        }
    }
    fn aggregate(
        values: Vec<Self>,
        id: String,
        report: &mut Report,
    ) -> Result<Self, TransformError> {
        chat::aggregate(values, id, report)
    }
}
impl Client for g::GenerateContentResponseBody {
    fn signed_calls(&self) -> Vec<bool> {
        self.candidates
            .iter()
            .flatten()
            .filter_map(|c| c.content.as_ref())
            .flat_map(|c| c.parts.iter().flatten())
            .filter(|p| p.function_call.is_some())
            .map(|p| p.thought_signature.is_some())
            .collect()
    }
    fn set_calls(&mut self, ids: Vec<Option<String>>) {
        for (call, id) in self
            .candidates
            .iter_mut()
            .flatten()
            .filter_map(|c| c.content.as_mut())
            .flat_map(|c| c.parts.iter_mut().flatten())
            .filter_map(|p| p.function_call.as_mut())
            .zip(ids)
        {
            call.id = id;
        }
    }
    fn aggregate(
        values: Vec<Self>,
        id: String,
        report: &mut Report,
    ) -> Result<Self, TransformError> {
        gemini::aggregate(values, id, report)
    }
}
/// Each child's tools keep their actual native association. The aggregate has
/// its own response identity and journal linking all actual native responses.
pub(super) struct ToolsOnly<'a, C>(pub &'a C);
impl<C: IdentityFacts> IdentityFacts for ToolsOnly<'_, C> {
    fn dialect(&self) -> Dialect {
        self.0.dialect()
    }
    fn response_id(&self) -> Option<&str> {
        None
    }
    fn native_model(&self) -> Option<&str> {
        self.0.native_model()
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        self.0.tools()
    }
}
pub(super) fn reserved<C: Client>(values: &[C]) -> Result<BTreeSet<String>, TransformError> {
    let mut ids = BTreeSet::new();
    for value in values {
        for (signed, tool) in value.signed_calls().into_iter().zip(value.tools()) {
            if signed
                && let Some(id) = tool.call_id
                && !ids.insert(id)
            {
                return Err(conflict(
                    "independent children repeat an immutable signed tool ID",
                ));
            }
        }
    }
    Ok(ids)
}
pub(super) fn normalize<N: IdentityFacts, C: Client>(
    native: &N,
    client: &mut C,
    identities: &GenerationIdentity,
    seen: &mut BTreeSet<String>,
    reserved: &BTreeSet<String>,
    max: usize,
    bindings: Option<&super::super::request_ids::SignedToolBindings>,
) -> Result<IdentityFlow, TransformError> {
    let originals = native.tools();
    let emitted = client.tools();
    let signed = client.signed_calls();
    if originals.len() != emitted.len()
        || originals.len() != signed.len()
        || originals
            .iter()
            .zip(&emitted)
            .any(|(a, b)| a.name != b.name || a.kind != b.kind)
    {
        return Err(TransformError::invalid_result(
            "fanout.calls",
            "native and converted tool ordering differ",
        ));
    }
    let mut native_seen = BTreeSet::new();
    for call in &originals {
        if let Some(id) = call.call_id.as_ref().filter(|id| !id.is_empty())
            && !native_seen.insert(id)
        {
            return Err(conflict("one native response repeats a tool call ID"));
        }
    }
    let mut flow = IdentityFlow::new(identities.response.namespace());
    let mut index = 0u64;
    let mut calls = Vec::new();
    for ((original, emitted), signed) in originals.into_iter().zip(emitted).zip(signed) {
        if signed {
            let evidence = bindings
                .ok_or_else(|| conflict("signed client call has no verified native restoration"))?;
            if let Some(id) = &emitted.call_id {
                if evidence.original_call_id(id) != Some(&original.call_id)
                    || !identities.response_policy.accepts_source(id)
                    || !seen.insert(id.clone())
                {
                    return Err(conflict(
                        "immutable signed call cannot retain its native association",
                    ));
                }
                if seen.len() > max {
                    return Err(limit());
                }
            }
            calls.push(emitted.call_id);
            continue;
        }
        if seen.len() >= max {
            return Err(limit());
        }
        let mut source = original
            .call_id
            .clone()
            .filter(|id| !seen.contains(id) && !reserved.contains(id));
        let handle = loop {
            if index > max as u64 {
                return Err(limit());
            }
            let mut trial = flow.clone();
            let mut handle = trial
                .resolve_or_allocate(
                    IdentityRole::ToolCall,
                    SourceIdentity::new(native.dialect(), source.clone(), index),
                    &identities.response_policy,
                )
                .map_err(|e| conflict(e.to_string()))?;
            index = index.checked_add(1).ok_or_else(limit)?;
            if seen.contains(&handle.emitted_id) || reserved.contains(&handle.emitted_id) {
                source = None;
                continue;
            }
            if handle.source_id().is_none()
                && let Some(id) = original.call_id
            {
                handle = trial
                    .attach_source(&handle, id)
                    .map_err(|e| conflict(e.to_string()))?;
            }
            flow = trial;
            break handle;
        };
        seen.insert(handle.emitted_id.clone());
        calls.push(Some(handle.emitted_id));
    }
    client.set_calls(calls);
    Ok(flow)
}

fn sum(a: i64, b: i64) -> Result<i64, TransformError> {
    if a < 0 || b < 0 {
        return Err(TransformError::invalid_result(
            "fanout.usage",
            "negative token count",
        ));
    }
    a.checked_add(b).ok_or_else(limit)
}
fn optional(
    a: Option<i64>,
    b: Option<i64>,
    field: &str,
    report: &mut Report,
) -> Result<Option<i64>, TransformError> {
    match (a, b) {
        (Some(a), Some(b)) => sum(a, b).map(Some),
        (None, None) => Ok(None),
        _ => {
            report.omitted(
                field,
                "some child calls lack this actual count; aggregate is unknown",
            );
            Ok(None)
        }
    }
}
fn common<T: PartialEq>(a: Option<T>, b: Option<T>, field: &str, report: &mut Report) -> Option<T> {
    if a == b {
        a
    } else {
        report.omitted(
            field,
            "child values differ; no single factual aggregate value",
        );
        None
    }
}
