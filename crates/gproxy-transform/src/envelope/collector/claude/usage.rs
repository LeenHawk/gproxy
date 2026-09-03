use gproxy_protocol::claude;

pub(super) fn merge_usage(target: &mut claude::Usage, update: claude::Usage) {
    target.input_tokens = update.input_tokens.or(target.input_tokens);
    target.output_tokens = update.output_tokens.or(target.output_tokens);
    target.cache_creation_input_tokens = update
        .cache_creation_input_tokens
        .or(target.cache_creation_input_tokens);
    target.cache_read_input_tokens = update
        .cache_read_input_tokens
        .or(target.cache_read_input_tokens);
    target.cache_creation = update.cache_creation.or(target.cache_creation.take());
    target.output_tokens_details = update
        .output_tokens_details
        .or(target.output_tokens_details.take());
    target.server_tool_use = update.server_tool_use.or(target.server_tool_use.take());
    target.iterations = update.iterations.or(target.iterations.take());
    target.inference_geo = update.inference_geo.or(target.inference_geo.take());
    target.service_tier = update.service_tier.or(target.service_tier.take());
    target.speed = update.speed.or(target.speed.take());
}
