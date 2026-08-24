use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::{ContentConverter, wire};

pub(super) fn convert(
    state: &mut ContentConverter,
    item: &openai::TypedResponseItem,
) -> Result<Option<gemini::Content>, TransformError> {
    let (call_id, item_id, code, rest) = match item {
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            id,
            rest,
            ..
        } => (
            call_id.clone(),
            id.clone(),
            action.commands.join("\n"),
            rest.clone(),
        ),
        openai::TypedResponseItem::LocalShellCall {
            action,
            call_id,
            id,
            rest,
            ..
        } => {
            state.native_ids.insert(id.clone(), call_id.clone());
            (
                call_id.clone(),
                Some(id.clone()),
                action.command.join("\n"),
                rest.clone(),
            )
        }
        openai::TypedResponseItem::ApplyPatchCall {
            call_id,
            operation,
            id,
            rest,
            ..
        } => (
            call_id.clone(),
            id.clone(),
            match operation {
                openai::ApplyPatchOperation::CreateFile { diff, .. }
                | openai::ApplyPatchOperation::UpdateFile { diff, .. } => diff.clone(),
                openai::ApplyPatchOperation::DeleteFile { .. } => serde_json::to_string(operation)?,
            },
            rest.clone(),
        ),
        openai::TypedResponseItem::CodeInterpreterCall { id, code, rest, .. } => (
            id.clone(),
            Some(id.clone()),
            code.clone().ok_or_else(|| {
                TransformError::shape("Responses code interpreter call", "code is missing")
            })?,
            rest.clone(),
        ),
        _ => return Ok(None),
    };
    Ok(Some(super::model_content(
        vec![gemini::Part {
            data: Some(gemini::PartData::ExecutableCode {
                executable_code: gemini::ExecutableCode {
                    id: Some(call_id),
                    language: gemini::ExecutableCodeLanguage::Known(
                        gemini::ExecutableCodeLanguageKnown::Python,
                    ),
                    code,
                    rest: Default::default(),
                },
                rest: Default::default(),
            }),
            rest: wire::openai_item_rest(rest, item_id),
            ..Default::default()
        }],
        Default::default(),
    )))
}
