use crate::dialects::shared::warnings::Walk;
use crate::*;
pub(crate) fn for_request(walk: &mut Walk, request: &GenerateRequest) {
    let options = &request.provider_options;
    if walk.is_responses()
        && request.settings.output_schema.is_some()
        && options
            .openai
            .responses
            .text
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_some()
    {
        walk.warn(
            WarningKind::Compatibility,
            "settings.output_schema",
            "Responses text.format overrides the portable schema",
        );
    }
}
