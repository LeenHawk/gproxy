use gproxy_channel_api::ModelInfo;

const MODELS: &[(&str, &str, i64, i64, Option<bool>)] = &[
    (
        "gemini-3.1-flash-image-preview",
        "Gemini 3.1 Flash Image Preview",
        32_768,
        32_768,
        None,
    ),
    (
        "gemini-3.1-pro-preview",
        "Gemini 3.1 Pro Preview",
        1_048_576,
        65_536,
        Some(true),
    ),
    (
        "gemini-3-pro-preview",
        "Gemini 3 Pro Preview",
        1_048_576,
        65_536,
        Some(true),
    ),
    (
        "gemini-3-pro-image-preview",
        "Nano Banana Pro",
        131_072,
        32_768,
        Some(true),
    ),
    (
        "gemini-3-flash-preview",
        "Gemini 3 Flash Preview",
        1_048_576,
        65_536,
        Some(true),
    ),
    (
        "gemini-2.5-pro",
        "Gemini 2.5 Pro",
        1_048_576,
        65_536,
        Some(true),
    ),
    (
        "gemini-2.5-flash",
        "Gemini 2.5 Flash",
        1_048_576,
        65_536,
        Some(true),
    ),
    (
        "gemini-2.5-flash-image",
        "Nano Banana",
        32_768,
        32_768,
        None,
    ),
    (
        "gemini-2.5-flash-lite-preview-09-2025",
        "Gemini 2.5 Flash-Lite Preview (09-2025)",
        1_048_576,
        65_536,
        Some(true),
    ),
    (
        "gemini-2.5-flash-lite",
        "Gemini 2.5 Flash-Lite",
        1_048_576,
        65_536,
        Some(true),
    ),
    (
        "gemini-2.0-flash-001",
        "Gemini 2.0 Flash 001",
        1_048_576,
        8_192,
        None,
    ),
    (
        "gemini-2.0-flash-lite-001",
        "Gemini 2.0 Flash-Lite 001",
        1_048_576,
        8_192,
        None,
    ),
];

pub(super) fn models() -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(
            |(id, display_name, context_window, max_output_tokens, thinking_supported)| ModelInfo {
                id: (*id).into(),
                display_name: Some((*display_name).into()),
                context_window: Some(*context_window),
                max_output_tokens: Some(*max_output_tokens),
                thinking_supported: *thinking_supported,
                thinking_adaptive_supported: None,
                thinking_enabled_supported: None,
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_catalog_keeps_v2_ids_and_limits() {
        let models = super::models();
        assert_eq!(models.len(), 12);
        let pro = models
            .iter()
            .find(|model| model.id == "gemini-3.1-pro-preview")
            .unwrap();
        assert_eq!(pro.context_window, Some(1_048_576));
        assert_eq!(pro.max_output_tokens, Some(65_536));
        assert_eq!(pro.thinking_supported, Some(true));
    }
}
