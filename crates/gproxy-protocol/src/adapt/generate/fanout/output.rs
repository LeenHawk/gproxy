use super::super::identity_facts::IdentityFacts;
use super::*;
use crate::{
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

pub(super) fn normalize<N: IdentityFacts, C: Client>(
    native: &N,
    client: &mut C,
    identities: &GenerationIdentity,
    seen: &mut BTreeSet<String>,
    max: usize,
) -> Result<(), TransformError> {
    let originals = native.tools();
    let emitted = client.tools();
    if originals.len() != emitted.len()
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
    for original in originals {
        if seen.len() >= max {
            return Err(limit());
        }
        // A candidate repeating an ID an earlier one already emitted gets
        // the counted alias for it, which still names the upstream ID.
        let avoid = seen.clone();
        let handle = flow
            .resolve_or_allocate_avoiding(
                IdentityRole::ToolCall,
                SourceIdentity::new(native.dialect(), original.call_id.clone(), index),
                &identities.response_policy,
                &avoid,
            )
            .map_err(|e| conflict(e.to_string()))?;
        index = index.checked_add(1).ok_or_else(limit)?;
        if avoid.contains(&handle.emitted_id) {
            return Err(conflict(
                "candidate call alias collides with another candidate",
            ));
        }
        seen.insert(handle.emitted_id.clone());
        calls.push(Some(handle.emitted_id));
    }
    client.set_calls(calls);
    Ok(())
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
