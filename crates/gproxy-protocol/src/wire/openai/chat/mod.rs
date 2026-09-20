pub mod reasoning;
pub use reasoning::{ReasoningDetail, ReasoningDetailKind, visible_reasoning};
pub mod content;
pub mod request;
pub mod response;
pub mod stream;
pub mod tools;

pub use content::*;
pub use request::*;
pub use response::*;
pub use stream::*;
pub use tools::*;

/// Select one provider spelling without duplicating reasoning when both are
/// present. Native passthrough retains both fields independently.
pub fn reasoning_text<'a>(
    content: &'a Option<Option<String>>,
    reasoning: &'a Option<Option<String>>,
) -> Option<&'a str> {
    content
        .as_ref()
        .and_then(Option::as_deref)
        .filter(|s| !s.is_empty())
        .or_else(|| reasoning.as_ref().and_then(Option::as_deref))
        .or_else(|| content.as_ref().and_then(Option::as_deref))
}
