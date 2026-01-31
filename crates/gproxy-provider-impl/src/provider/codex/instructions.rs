const BASE_INSTRUCTIONS: &str = include_str!("instructions/prompt.md");
const BASE_INSTRUCTIONS_WITH_APPLY_PATCH: &str =
    include_str!("instructions/prompt_with_apply_patch_instructions.md");

const GPT_5_CODEX_INSTRUCTIONS: &str = include_str!("instructions/gpt_5_codex_prompt.md");
const GPT_5_1_INSTRUCTIONS: &str = include_str!("instructions/gpt_5_1_prompt.md");
const GPT_5_2_INSTRUCTIONS: &str = include_str!("instructions/gpt_5_2_prompt.md");
const GPT_5_1_CODEX_MAX_INSTRUCTIONS: &str =
    include_str!("instructions/gpt-5.1-codex-max_prompt.md");
const GPT_5_2_CODEX_INSTRUCTIONS: &str =
    include_str!("instructions/gpt-5.2-codex_prompt.md");

const GPT_5_2_CODEX_TEMPLATE: &str =
    include_str!("instructions/gpt-5.2-codex_instructions_template.md");
const GPT_5_2_CODEX_PERSONALITY_FRIENDLY: &str =
    include_str!("instructions/gpt-5.2-codex_friendly.md");
const GPT_5_2_CODEX_PERSONALITY_PRAGMATIC: &str =
    include_str!("instructions/gpt-5.2-codex_pragmatic.md");

const PERSONALITY_PLACEHOLDER: &str = "{{ personality }}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexPersonality {
    Friendly,
    Pragmatic,
}

pub fn parse_personality(value: &str) -> Option<CodexPersonality> {
    match value.trim().to_ascii_lowercase().as_str() {
        "friendly" => Some(CodexPersonality::Friendly),
        "pragmatic" => Some(CodexPersonality::Pragmatic),
        _ => None,
    }
}

pub fn instructions_for_model(model: &str, personality: Option<CodexPersonality>) -> String {
    if model.starts_with("o3") || model.starts_with("o4-mini") {
        BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string()
    } else if model.starts_with("codex-mini-latest") {
        BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string()
    } else if model.starts_with("gpt-4.1") {
        BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string()
    } else if model.starts_with("gpt-4o") {
        BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string()
    } else if model.starts_with("gpt-3.5") {
        BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string()
    } else if model.starts_with("test-gpt-5") {
        GPT_5_CODEX_INSTRUCTIONS.to_string()
    } else if model.starts_with("gpt-5.2-codex") || model.starts_with("bengalfox") {
        let personality_text = match personality {
            Some(CodexPersonality::Friendly) => GPT_5_2_CODEX_PERSONALITY_FRIENDLY,
            Some(CodexPersonality::Pragmatic) => GPT_5_2_CODEX_PERSONALITY_PRAGMATIC,
            None => "",
        };
        let rendered = GPT_5_2_CODEX_TEMPLATE.replace(PERSONALITY_PLACEHOLDER, personality_text);
        if rendered.trim().is_empty() {
            GPT_5_2_CODEX_INSTRUCTIONS.to_string()
        } else {
            rendered
        }
    } else if model.starts_with("gpt-5.1-codex-max") {
        GPT_5_1_CODEX_MAX_INSTRUCTIONS.to_string()
    } else if (model.starts_with("gpt-5-codex")
        || model.starts_with("gpt-5.1-codex")
        || model.starts_with("codex-"))
        && !model.contains("-mini")
    {
        GPT_5_CODEX_INSTRUCTIONS.to_string()
    } else if model.starts_with("gpt-5-codex")
        || model.starts_with("gpt-5.1-codex")
        || model.starts_with("codex-")
    {
        GPT_5_CODEX_INSTRUCTIONS.to_string()
    } else if model.starts_with("gpt-5.2") || model.starts_with("boomslang") {
        GPT_5_2_INSTRUCTIONS.to_string()
    } else if model.starts_with("gpt-5.1") {
        GPT_5_1_INSTRUCTIONS.to_string()
    } else if model.starts_with("gpt-5") {
        BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string()
    } else {
        BASE_INSTRUCTIONS.to_string()
    }
}
