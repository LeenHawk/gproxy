use crate::{
    transform::TransformError,
    wire::{
        DeclaredFields,
        openai::{compact::ClientCompactResponseBody, guardian as c},
    },
};
/// Construct the exact client response envelope consumed by CompactClient.
/// The caller replaces history with this full array; no native encrypted block
/// is invented and retained tail items are preserved in their original order.
pub fn replacement_history(
    summary: Option<String>,
    retained_tail: Vec<c::ClientResponseItem>,
) -> Result<ClientCompactResponseBody, TransformError> {
    let mut output = Vec::with_capacity(retained_tail.len() + usize::from(summary.is_some()));
    if let Some(summary) = summary {
        if summary.trim().is_empty() {
            return Err(TransformError::invalid_result(
                "compact.summary",
                "empty summary",
            ));
        }
        output.push(c::ClientResponseItem::Message(
            c::ClientMessage::builder(
                "assistant".into(),
                vec![c::ContentItem::OutputText(
                    c::ContentItemOutputText::builder(summary).build(),
                )],
            )
            .build(),
        ));
    }
    output.extend(retained_tail.into_declared());
    Ok(ClientCompactResponseBody::builder(output).build())
}
