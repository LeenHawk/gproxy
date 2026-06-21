use crate::protocol::claude;

pub(in crate::transform::count_tokens) fn claude_previous_message_id_to_openai(
    diagnostics: Option<claude::DiagnosticsParam>,
) -> Option<String> {
    diagnostics?.previous_message_id?
}

pub(in crate::transform::count_tokens) fn openai_previous_response_id_to_claude(
    previous_response_id: Option<String>,
) -> Option<claude::DiagnosticsParam> {
    Some(claude::DiagnosticsParam {
        previous_message_id: Some(Some(previous_response_id?)),
        extra: Default::default(),
    })
}
