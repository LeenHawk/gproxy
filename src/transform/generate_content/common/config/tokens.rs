pub(in crate::transform::generate_content) fn merge_openai_max_tokens(
    max_completion_tokens: Option<u32>,
    max_tokens: Option<u32>,
) -> Option<u32> {
    max_completion_tokens.or(max_tokens)
}
