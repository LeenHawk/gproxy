use crate::protocol::{claude, openai};

pub(super) fn response_usage_to_claude(usage: Option<openai::ResponseUsage>) -> claude::Usage {
    let Some(usage) = usage else {
        return empty_usage();
    };
    let (cached_tokens, cache_write_tokens) = usage
        .input_tokens_details
        .map(|details| (details.cached_tokens, details.cache_write_tokens))
        .unwrap_or_default();
    let input_tokens = usage
        .input_tokens
        .saturating_sub(cached_tokens)
        .saturating_sub(cache_write_tokens);
    let reasoning_tokens = usage.output_tokens_details.reasoning_tokens;

    crate::protocol::wire!(claude::Usage {
        input_tokens: Some(u64::from(input_tokens)),
        output_tokens: Some(u64::from(usage.output_tokens)),
        cache_creation_input_tokens: (cache_write_tokens > 0)
            .then_some(u64::from(cache_write_tokens)),
        cache_read_input_tokens: (cached_tokens > 0).then_some(u64::from(cached_tokens)),
        cache_creation: None,
        output_tokens_details: (reasoning_tokens > 0).then(|| crate::protocol::wire!(
            claude::OutputTokensDetails {
                thinking_tokens: u64::from(reasoning_tokens),
                extra: Default::default(),
            }
        )),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    })
}

fn empty_usage() -> claude::Usage {
    crate::protocol::wire!(claude::Usage {
        input_tokens: Some(0),
        output_tokens: Some(0),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        output_tokens_details: None,
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    })
}
