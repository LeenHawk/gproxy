use super::*;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn reasoning_maps_levels_budgets_defaults_and_vendor_overrides() {
    for (effort, wire) in [
        (ReasoningEffort::None, "none"),
        (ReasoningEffort::Minimal, "minimal"),
        (ReasoningEffort::Low, "low"),
        (ReasoningEffort::Medium, "medium"),
        (ReasoningEffort::High, "high"),
        (ReasoningEffort::Xhigh, "xhigh"),
        (ReasoningEffort::Max, "max"),
    ] {
        for dialect in [Dialect::Openai, Dialect::OpenaiChat] {
            let mut request = support::request(dialect, vec![]);
            request.settings.reasoning = Some(ReasoningSettings {
                effort: effort.clone(),
                token_budget: None,
            });
            let native = dialects::render(&request, false).unwrap().request;
            assert_eq!(
                if dialect == Dialect::Openai {
                    &native["reasoning"]["effort"]
                } else {
                    &native["reasoning_effort"]
                },
                wire
            );
        }
    }
    let mut claude = support::request(Dialect::Claude, vec![]);
    claude.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::Minimal,
        token_budget: None,
    });
    let compiled = dialects::render(&claude, false).unwrap();
    assert_eq!(compiled.request["thinking"]["type"], "adaptive");
    assert_eq!(compiled.request["output_config"]["effort"], "low");
    assert!(!compiled.warnings.is_empty());
    claude.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::ProviderDefault,
        token_budget: Some(2048),
    });
    let native = dialects::render(&claude, false).unwrap().request;
    assert_eq!(native["thinking"]["budget_tokens"], 2048);
    claude.provider_options.claude.thinking =
        Some(serde_json::from_value(json!({"type":"disabled"})).unwrap());
    let compiled = dialects::render(&claude, false).unwrap();
    assert_eq!(compiled.request["thinking"]["type"], "disabled");
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning")
    );
    let mut gemini = support::request(Dialect::Gemini, vec![]);
    gemini.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::Max,
        token_budget: None,
    });
    let compiled = dialects::render(&gemini, false).unwrap();
    assert_eq!(
        compiled.request["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "HIGH"
    );
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning.effort")
    );
    gemini.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::None,
        token_budget: None,
    });
    assert_eq!(
        dialects::render(&gemini, false).unwrap().request["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        0
    );
    let mut openai = support::request(Dialect::Openai, vec![]);
    openai.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::High,
        token_budget: Some(12),
    });
    openai.provider_options.openai.responses.reasoning = Some(Some(
        serde_json::from_value(json!({"effort":"low"})).unwrap(),
    ));
    let compiled = dialects::render(&openai, false).unwrap();
    assert_eq!(compiled.request["reasoning"]["effort"], "low");
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning.token_budget")
    );
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning.effort")
    );
    openai.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::ProviderDefault,
        token_budget: None,
    });
    openai.provider_options = ProviderOptions::default();
    assert!(
        dialects::render(&openai, false)
            .unwrap()
            .request
            .get("reasoning")
            .is_none()
    );
}
