use crate::dialects::shared::warnings::Walk;
use crate::*;
pub(crate) fn for_request(walk: &mut Walk, request: &GenerateRequest) {
    let options = &request.provider_options;
    if walk.is_chat() {
        if request.settings.speech.is_some()
            && options
                .openai
                .chat
                .audio
                .as_ref()
                .and_then(Option::as_ref)
                .is_some()
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.speech",
                "Chat audio options override portable speech settings",
            );
        }
        if request.settings.output_schema.is_some() && options.openai.chat.response_format.is_some()
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.output_schema",
                "Chat response_format overrides the portable schema",
            );
        }
    }
}
