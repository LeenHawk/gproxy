pub(crate) fn gpt_encoding(model: &str) -> Option<&'static tiktoken_rs::CoreBPE> {
    let base = model.strip_prefix("ft:").unwrap_or(model);
    let is_gpt = base.starts_with("gpt-")
        || base.starts_with("chatgpt-")
        || base.starts_with("codex-mini")
        || matches!(base, "o1" | "o3" | "o4")
        || ["o1-", "o3-", "o4-"]
            .iter()
            .any(|prefix| base.starts_with(prefix));
    if !is_gpt {
        return None;
    }
    // Known models use tiktoken's mapping (including GPT-OSS). New GPT names
    // continue using tiktoken with the modern default vocabulary.
    Some(tiktoken_rs::bpe_for_model(model).unwrap_or_else(|_| tiktoken_rs::o200k_base_singleton()))
}
