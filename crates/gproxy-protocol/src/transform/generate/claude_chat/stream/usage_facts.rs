use super::*;

/// Input/cache facts describe the fixed input of this invocation. They may fill
/// omitted native details, but must not contradict the final native counters.
pub(super) fn resolve(
    native: Option<&o::Usage>,
    facts: Option<c::Usage>,
    initial: Option<&c::Usage>,
    report: &mut Report,
) -> Result<c::Usage, TransformError> {
    let mut facts = facts.into_declared();
    if let (Some(facts), Some(initial)) = (&mut facts, initial) {
        let known = super::super::usage::to_chat(initial, &mut Report::default())?;
        if let Some(details) = known.prompt_tokens_details {
            merge_nested(&mut facts.cache_read_input_tokens, details.cached_tokens)?;
            merge_nested(
                &mut facts.cache_creation_input_tokens,
                details.cache_write_tokens,
            )?;
        }
    }
    let mut output = match (native, facts) {
        (Some(native), None) => {
            let mut native = native.clone();
            if let Some(initial) = initial {
                let known = super::super::usage::to_chat(initial, &mut Report::default())?;
                if let Some(details) = known.prompt_tokens_details {
                    let target = native
                        .prompt_tokens_details
                        .get_or_insert_with(|| o::PromptTokensDetails::builder().build());
                    merge(&mut target.cached_tokens, details.cached_tokens)?;
                    merge(&mut target.cache_write_tokens, details.cache_write_tokens)?;
                }
            }
            super::super::usage::to_claude(&native, report)?
        }
        (None, Some(facts)) => facts.into_declared(),
        (Some(native), Some(facts)) => {
            let mut facts = facts.into_declared();
            if let Some(details) = &native.prompt_tokens_details {
                merge_nested(&mut facts.cache_read_input_tokens, details.cached_tokens)?;
                merge_nested(
                    &mut facts.cache_creation_input_tokens,
                    details.cache_write_tokens,
                )?;
            }
            if let Some(thinking) = native
                .completion_tokens_details
                .as_ref()
                .and_then(|v| v.reasoning_tokens)
            {
                if facts
                    .output_tokens_details
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|v| v.thinking_tokens != thinking)
                {
                    return Err(invalid("final reasoning facts contradict native usage"));
                }
                facts.output_tokens_details =
                    Some(Some(c::OutputTokensDetails::builder(thinking).build()));
            }
            let back = super::super::usage::to_chat(&facts, &mut Report::default())?;
            if back.prompt_tokens != native.prompt_tokens
                || back.completion_tokens != native.completion_tokens
                || back.total_tokens != native.total_tokens
            {
                return Err(invalid("final usage facts contradict native totals"));
            }
            if facts.cache_creation_input_tokens.flatten().is_none() {
                facts.cache_creation_input_tokens = back
                    .prompt_tokens_details
                    .and_then(|v| v.cache_write_tokens)
                    .map(Some);
            }
            facts
        }
        (None, None) => return Err(TransformError::missing_metadata("final usage")),
    };
    if output.cache_creation_input_tokens.flatten().is_none() {
        let normalized = super::super::usage::to_chat(&output, &mut Report::default())?;
        output.cache_creation_input_tokens = normalized
            .prompt_tokens_details
            .and_then(|v| v.cache_write_tokens)
            .map(Some);
    }
    if let Some(initial) = initial {
        if output.input_tokens != initial.input_tokens {
            return Err(invalid(
                "final input count contradicts factual initial input",
            ));
        }
        let a = super::super::usage::to_chat(initial, &mut Report::default())?;
        let b = super::super::usage::to_chat(&output, &mut Report::default())?;
        if let (Some(a), Some(b)) = (a.prompt_tokens_details, b.prompt_tokens_details) {
            let mut cached = b.cached_tokens;
            merge(&mut cached, a.cached_tokens)?;
            let mut written = b.cache_write_tokens;
            merge(&mut written, a.cache_write_tokens)?;
        }
    }
    Ok(output)
}

fn merge(actual: &mut Option<i64>, known: Option<i64>) -> Result<(), TransformError> {
    if let (Some(a), Some(b)) = (*actual, known)
        && a != b
    {
        return Err(invalid("usage cache facts contradict native counters"));
    }
    if actual.is_none() {
        *actual = known;
    }
    Ok(())
}

fn merge_nested(
    actual: &mut Option<Option<i64>>,
    known: Option<i64>,
) -> Result<(), TransformError> {
    let mut value = actual.flatten();
    merge(&mut value, known)?;
    if value.is_some() {
        *actual = value.map(Some);
    }
    Ok(())
}
