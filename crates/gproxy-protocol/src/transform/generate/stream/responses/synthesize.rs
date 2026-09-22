use super::{emit::Emitter, *};
use crate::{
    transform::{
        Converted, Report,
        identity::{IdentityFlow, IdentityRole, OutputItemKind, SourceIdentity, TargetIdPolicy},
    },
    wire::DeclaredFields,
};

/// Synthesize a complete native lifecycle. Caller-owned identity flow supplies
/// missing output item IDs independently of tool call IDs.
pub fn synthesize_responses_stream(
    input: r::GenerateContentResponseBody,
    flow: &mut IdentityFlow,
    limits: ResponsesStreamLimits,
) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
    let mut input = input.into_declared();
    bounded(&input, limits.max_bytes)?;
    if !matches!(
        input.status,
        Some(r::ResponseStatus::Completed | r::ResponseStatus::Incomplete)
    ) {
        return Err(invalid(
            "only completed/incomplete native responses can synthesize terminal streams",
        ));
    }
    let mut transaction = flow.clone();
    let policy = TargetIdPolicy::new(crate::Dialect::OpenAi);
    // Reserve existing function/custom item identities before allocating missing
    // ones, so a user-supplied ID cannot collide with a generated flow identity.
    for missing in [false, true] {
        for (index, item) in input.output.iter_mut().enumerate() {
            let (id, kind) = match item {
                r::ResponseOutputItem::FunctionCall(v) => (&mut v.id, OutputItemKind::FunctionCall),
                r::ResponseOutputItem::CustomToolCall(v) => {
                    (&mut v.id, OutputItemKind::CustomToolCall)
                }
                _ => continue,
            };
            if id.is_none() != missing {
                continue;
            }
            *id = Some(
                transaction
                    .resolve_or_allocate(
                        IdentityRole::OutputItem(kind),
                        SourceIdentity {
                            dialect: crate::Dialect::OpenAi,
                            source_id: id.clone(),
                            logical_index: index as u64,
                        },
                        &policy,
                    )
                    .map_err(|e| {
                        TransformError::with_source(
                            crate::transform::TransformErrorKind::Conflict,
                            "response.output.id",
                            "cannot allocate output identity",
                            e,
                        )
                    })?
                    .emitted_id,
            );
        }
    }
    let mut emitter = Emitter::new(limits);
    let mut created = input.clone();
    created.output.clear();
    created.output_text = None;
    created.status = Some(r::ResponseStatus::InProgress);
    created.error = None;
    created.incomplete_details = None;
    created.completed_at = None;
    created.usage = None;
    created.prompt_cache_diagnostics = None;
    emitter.push(|sequence_number| {
        s::StreamEvent::Created(s::ResponseCreated {
            sequence_number,
            response: created,
            rest: Default::default(),
        })
    })?;
    for (index, item) in input.output.iter().enumerate() {
        emitter.item(index as i64, item)?;
    }
    emitter.push(|sequence_number| match input.status {
        Some(r::ResponseStatus::Incomplete) => s::StreamEvent::Incomplete(s::ResponseIncomplete {
            sequence_number,
            response: input,
            rest: Default::default(),
        }),
        _ => s::StreamEvent::Completed(s::ResponseCompleted {
            sequence_number,
            response: input,
            rest: Default::default(),
        }),
    })?;
    emitter.finish()?;
    *flow = transaction;
    Ok(Converted {
        value: emitter.events,
        report: Report::default(),
    })
}
