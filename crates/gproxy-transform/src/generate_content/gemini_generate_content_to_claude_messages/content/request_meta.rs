use gproxy_protocol::{claude, gemini};

use crate::TransformError;

pub(crate) fn system(
    content: gemini::Content,
) -> Result<Option<claude::SystemPrompt>, TransformError> {
    let gemini::Content { parts, role, .. } = content;
    if !matches!(
        role,
        None | Some(gemini::ContentRole::Known(
            gemini::ContentRoleKnown::System | gemini::ContentRoleKnown::User
        ))
    ) {
        return Err(TransformError::unsupported(
            "Gemini system instruction",
            "content role or metadata",
        ));
    }
    let blocks = parts
        .into_iter()
        .map(system_part)
        .collect::<Result<Vec<_>, TransformError>>()?;
    Ok(Some(claude::StringOrArray::Array(blocks)))
}

pub(super) fn role(
    role: Option<gemini::ContentRole>,
) -> Result<claude::MessageRole, TransformError> {
    match role {
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)) => Ok(
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
        ),
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)) => {
            Ok(claude::MessageRole::Known(claude::MessageRoleKnown::System))
        }
        Some(gemini::ContentRole::Known(
            gemini::ContentRoleKnown::User | gemini::ContentRoleKnown::Function,
        ))
        | None => Ok(claude::MessageRole::Known(claude::MessageRoleKnown::User)),
        Some(gemini::ContentRole::Unknown(value)) => {
            Err(TransformError::unsupported("Gemini role", value))
        }
        _ => Err(TransformError::unsupported("Gemini role", "future role")),
    }
}

fn system_part(part: gemini::Part) -> Result<claude::TextBlock, TransformError> {
    if part.thought.is_some()
        || part.thought_signature.is_some()
        || part.part_metadata.is_some()
        || part.media_resolution.is_some()
        || part.metadata.is_some()
    {
        return Err(TransformError::unsupported(
            "Gemini system part",
            "part metadata",
        ));
    }
    let Some(gemini::PartData::Text { text, .. }) = part.data else {
        return Err(TransformError::unsupported(
            "Gemini system instruction",
            "non-text part",
        ));
    };
    Ok(claude::TextBlock {
        text,
        type_: claude::TextBlockType::Text,
        cache_control: None,
        citations: None,
        rest: Default::default(),
    })
}
