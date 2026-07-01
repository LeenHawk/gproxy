/// Coarse role category used only for mechanical routing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleFamily {
    Instruction,
    User,
    Assistant,
    Tool,
}

pub fn openai_role_family(role: &str) -> Option<RoleFamily> {
    match role {
        "system" | "developer" => Some(RoleFamily::Instruction),
        "user" => Some(RoleFamily::User),
        "assistant" => Some(RoleFamily::Assistant),
        "tool" | "function" => Some(RoleFamily::Tool),
        _ => None,
    }
}

pub fn claude_role_family(role: &str) -> Option<RoleFamily> {
    match role {
        "user" => Some(RoleFamily::User),
        "assistant" => Some(RoleFamily::Assistant),
        _ => None,
    }
}

pub fn gemini_role_family(role: &str) -> Option<RoleFamily> {
    match role {
        "user" => Some(RoleFamily::User),
        "model" => Some(RoleFamily::Assistant),
        "function" => Some(RoleFamily::Tool),
        _ => None,
    }
}
