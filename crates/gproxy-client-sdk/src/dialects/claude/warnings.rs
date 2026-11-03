use crate::dialects::shared::warnings::Walk;
use crate::*;
pub(crate) fn for_request(walk: &mut Walk, request: &GenerateRequest) {
    let options = &request.provider_options;
    if walk.vendor == Vendor::Claude {
        if request.settings.temperature.is_some() || options.claude.rest.contains_key("temperature")
        {
            walk.warn(
                WarningKind::Deprecated,
                "settings.temperature",
                "Claude temperature is sent as a legacy compatibility extension",
            );
        }
        if request.settings.top_p.is_some() || options.claude.rest.contains_key("top_p") {
            walk.warn(
                WarningKind::Deprecated,
                "settings.top_p",
                "Claude top_p is sent as a legacy compatibility extension",
            );
        }
        if request.settings.speech.is_some() {
            walk.warn(
                WarningKind::Unsupported,
                "settings.speech",
                "Claude Messages has no speech-output settings",
            );
        }
        if request.settings.max_output_tokens.is_some() && options.claude.max_tokens.is_some() {
            walk.warn(
                WarningKind::Compatibility,
                "settings.max_output_tokens",
                "Claude max_tokens overrides the portable setting",
            );
        }
        if request.settings.output_schema.is_some()
            && options
                .claude
                .output_config
                .as_ref()
                .and_then(|c| c.format.as_ref())
                .is_some()
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.output_schema",
                "Claude output_config.format overrides the portable schema",
            );
        }
    }
}
