use gproxy_protocol::Rest;
use gproxy_protocol::transform::models::*;
use gproxy_protocol::wire::{
    claude::models as claude_models, gemini::models as gemini_models,
    openai::models as openai_models,
};
use std::collections::BTreeMap;

fn rest() -> Rest {
    let mut rest = Rest::new();
    rest.insert("foreign".into(), serde_json::json!(true));
    rest
}

fn caps() -> ClaudeCapabilities {
    ClaudeCapabilities {
        batch: true,
        citations: false,
        code_execution: false,
        context_management: ClaudeContextManagement {
            clear_thinking_20251015: false,
            clear_tool_uses_20250919: false,
            compact_20260112: true,
            supported: true,
        },
        effort: ClaudeEffort {
            high: true,
            low: true,
            max: false,
            medium: true,
            supported: true,
            xhigh: false,
        },
        image_input: true,
        pdf_input: false,
        structured_outputs: true,
        thinking: ClaudeThinking {
            supported: true,
            adaptive: true,
            enabled: true,
        },
    }
}

fn claude_supplement() -> ClaudeModelSupplement {
    ClaudeModelSupplement {
        display_name: Some("Model display".into()),
        allowed_fallback_models: vec!["fallback".into()],
        capabilities: caps(),
        max_input_tokens: Some(1000),
        max_tokens: Some(100),
        created_at: Some("2024-01-01T00:00:00Z".into()),
    }
}

fn claude_model() -> claude_models::ModelInfo {
    {
        let mut fixture = claude_models::ModelInfo::builder(
            "claude-1".into(),
            vec!["source-fallback".into()],
            {
                let mut fixture = claude_models::ModelCapabilities::builder(
                    {
                        let mut fixture = claude_models::CapabilitySupport::builder(true).build();
                        fixture.rest = rest();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::CapabilitySupport::builder(false).build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::CapabilitySupport::builder(false).build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::ContextManagementCapability::builder(
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                            false,
                        )
                        .build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::EffortCapability::builder(
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                            false,
                            {
                                let mut fixture =
                                    claude_models::CapabilitySupport::builder(false).build();
                                fixture.rest = Rest::new();
                                fixture
                            },
                        )
                        .build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::CapabilitySupport::builder(false).build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::CapabilitySupport::builder(false).build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::CapabilitySupport::builder(false).build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                    {
                        let mut fixture = claude_models::ThinkingCapability::builder(false, {
                            let mut fixture = claude_models::ThinkingTypes::builder(
                                {
                                    let mut fixture =
                                        claude_models::CapabilitySupport::builder(false).build();
                                    fixture.rest = Rest::new();
                                    fixture
                                },
                                {
                                    let mut fixture =
                                        claude_models::CapabilitySupport::builder(false).build();
                                    fixture.rest = Rest::new();
                                    fixture
                                },
                            )
                            .build();
                            fixture.rest = Rest::new();
                            fixture
                        })
                        .build();
                        fixture.rest = Rest::new();
                        fixture
                    },
                )
                .build();
                fixture.rest = Rest::new();
                fixture
            },
            "2024-01-01T00:00:00Z".into(),
            "Source display".into(),
            1000,
            100,
            claude_models::ModelType::Model,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    }
}

fn openai_model() -> openai_models::Model {
    {
        let mut fixture = openai_models::Model::builder(
            "openai-1".into(),
            1_704_067_200,
            openai_models::ModelObject::Model,
            "owner".into(),
        )
        .build();
        fixture.rest = rest();
        fixture
    }
}

fn gemini_model() -> gemini_models::Model {
    {
        let mut fixture =
            gemini_models::Model::builder("models/gemini-1".into(), "base-1".into(), "001".into())
                .build();
        fixture.display_name = Some("Gemini display".into());
        fixture.description = Some("description".into());
        fixture.input_token_limit = Some(1000);
        fixture.output_token_limit = Some(100);
        fixture.supported_generation_methods = Some(vec!["generateContent".into()]);
        fixture.thinking = Some(true);
        fixture.temperature = Some(0.5);
        fixture.max_temperature = Some(1.0);
        fixture.top_p = Some(0.9);
        fixture.top_k = Some(40);
        fixture.rest = rest();
        fixture
    }
}

fn openai_supplement() -> OpenAiModelSupplement {
    OpenAiModelSupplement {
        owned_by: "gateway".into(),
        created: Some(1_704_067_200),
    }
}

fn gemini_supplement() -> GeminiModelSupplement {
    GeminiModelSupplement {
        base_model_id: "base-supplied".into(),
        version: "v1".into(),
    }
}

fn gemini_claude_supplement() -> ClaudeModelSupplement {
    let mut supplement = claude_supplement();
    supplement.display_name = None;
    supplement
}

fn one<T>(key: &str, value: T) -> BTreeMap<String, T> {
    [(key.to_owned(), value)].into_iter().collect()
}

#[test]
fn all_six_get_directions_map_known_fields() {
    let claude_to_openai = claude_to_openai(claude_model(), &openai_supplement())
        .unwrap()
        .value;
    assert_eq!(claude_to_openai.id, "claude-1");
    assert_eq!(claude_to_openai.created, 1_704_067_200);
    let claude_to_gemini = claude_to_gemini(claude_model(), &gemini_supplement())
        .unwrap()
        .value;
    assert_eq!(claude_to_gemini.name, "models/claude-1");
    assert_eq!(claude_to_gemini.base_model_id, "base-supplied");

    let openai_to_claude = openai_to_claude(openai_model(), &claude_supplement())
        .unwrap()
        .value;
    assert_eq!(openai_to_claude.id, "openai-1");
    assert_eq!(openai_to_claude.created_at, "2024-01-01T00:00:00Z");
    let openai_to_gemini = openai_to_gemini(openai_model(), &gemini_supplement())
        .unwrap()
        .value;
    assert_eq!(openai_to_gemini.name, "models/openai-1");

    let gemini_to_openai = gemini_to_openai(gemini_model(), &openai_supplement())
        .unwrap()
        .value;
    assert_eq!(gemini_to_openai.id, "gemini-1");
    let mut gemini_claude_supplement = claude_supplement();
    gemini_claude_supplement.display_name = None;
    let gemini_to_claude = gemini_to_claude(gemini_model(), &gemini_claude_supplement)
        .unwrap()
        .value;
    assert_eq!(gemini_to_claude.id, "gemini-1");
    assert_eq!(gemini_to_claude.display_name, "Gemini display");
}

#[test]
fn source_rest_is_not_tunneled_and_nested_rest_is_empty() {
    let converted = claude_to_gemini(claude_model(), &gemini_supplement())
        .unwrap()
        .value;
    assert!(converted.rest.is_empty());
    let converted = openai_to_claude(openai_model(), &claude_supplement())
        .unwrap()
        .value;
    assert!(converted.rest.is_empty());
    assert!(converted.capabilities.rest.is_empty());
    assert!(converted.capabilities.batch.rest.is_empty());
}

#[test]
fn required_metadata_and_time_fail_without_fabrication() {
    let missing = openai_to_gemini(
        openai_model(),
        &GeminiModelSupplement {
            base_model_id: String::new(),
            version: String::new(),
        },
    );
    assert!(missing.is_err());
    let missing = gemini_to_openai(
        gemini_model(),
        &OpenAiModelSupplement {
            owned_by: String::new(),
            created: None,
        },
    );
    assert!(missing.is_err());
    let bad_time = claude_to_openai(
        {
            let mut value = claude_model();
            value.created_at = "not-a-time".into();
            value
        },
        &openai_supplement(),
    );
    assert!(bad_time.is_err());
    let mut fractional = claude_supplement();
    fractional.created_at = Some("2024-01-01T00:00:00.500Z".into());
    assert!(openai_to_claude(openai_model(), &fractional).is_err());
}

#[test]
fn pagination_requires_facts_and_complete_empty_list_needs_ids() {
    let body = {
        let mut fixture = openai_models::ListModelsResponseBody::builder(
            vec![openai_model()],
            openai_models::ListObject::List,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let missing = openai_to_claude_list(
        body.clone(),
        &one("openai-1", claude_supplement()),
        &ListPageFacts::default(),
    );
    assert!(missing.is_err());
    let converted = openai_to_claude_list(
        body,
        &one("openai-1", claude_supplement()),
        &ListPageFacts::complete(),
    )
    .unwrap()
    .value;
    assert_eq!(converted.first_id, "openai-1");
    assert!(!converted.has_more);

    let empty = {
        let mut fixture =
            openai_models::ListModelsResponseBody::builder(vec![], openai_models::ListObject::List)
                .build();
        fixture.rest = Rest::new();
        fixture
    };
    assert!(openai_to_claude_list(empty, &BTreeMap::new(), &ListPageFacts::complete()).is_err());
}

#[test]
fn pagination_facts_conflict_with_derived_ids() {
    let body = {
        let mut fixture = openai_models::ListModelsResponseBody::builder(
            vec![openai_model()],
            openai_models::ListObject::List,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let page = ListPageFacts::explicit("wrong", "openai-1", false);
    assert!(openai_to_claude_list(body, &one("openai-1", claude_supplement()), &page).is_err());
}

#[test]
fn all_list_directions_require_source_and_target_pagination_facts() {
    let claude_body = {
        let mut fixture = claude_models::ListModelsResponseBody::builder(
            vec![claude_model()],
            "claude-1".into(),
            false,
            "claude-1".into(),
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let openai_body = {
        let mut fixture = openai_models::ListModelsResponseBody::builder(
            vec![openai_model()],
            openai_models::ListObject::List,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let gemini_body = {
        let mut fixture = gemini_models::ListModelsResponseBody::builder().build();
        fixture.models = Some(vec![gemini_model()]);
        fixture.next_page_token = None;
        fixture.rest = Rest::new();
        fixture
    };
    let complete = ListPageFacts::complete();

    assert!(
        claude_to_openai_list(
            claude_body.clone(),
            &one("claude-1", openai_supplement()),
            &complete
        )
        .is_ok()
    );
    assert!(
        claude_to_gemini_list(
            claude_body.clone(),
            &one("claude-1", gemini_supplement()),
            &complete
        )
        .is_ok()
    );
    assert!(
        openai_to_claude_list(
            openai_body.clone(),
            &one("openai-1", claude_supplement()),
            &complete
        )
        .is_ok()
    );
    assert!(
        openai_to_gemini_list(
            openai_body.clone(),
            &one("openai-1", gemini_supplement()),
            &complete
        )
        .is_ok()
    );
    assert!(
        gemini_to_openai_list(
            gemini_body.clone(),
            &one("models/gemini-1", openai_supplement()),
            &complete
        )
        .is_ok()
    );
    assert!(
        gemini_to_claude_list(
            gemini_body.clone(),
            &one("models/gemini-1", gemini_claude_supplement()),
            &complete
        )
        .is_ok()
    );

    let mut truncated_claude = claude_body;
    truncated_claude.has_more = true;
    assert!(
        claude_to_openai_list(
            truncated_claude.clone(),
            &one("claude-1", openai_supplement()),
            &complete
        )
        .is_ok()
    );
    assert!(
        claude_to_openai_list(
            truncated_claude.clone(),
            &one("claude-1", openai_supplement()),
            &ListPageFacts::default().with_next_page_token("target-next")
        )
        .is_ok()
    );
    assert!(
        claude_to_gemini_list(
            truncated_claude,
            &one("claude-1", gemini_supplement()),
            &complete
        )
        .is_ok()
    );

    let mut paged_gemini = gemini_body;
    paged_gemini.next_page_token = Some("next".into());
    assert!(
        gemini_to_openai_list(
            paged_gemini.clone(),
            &one("models/gemini-1", openai_supplement()),
            &complete
        )
        .is_ok()
    );
    assert!(
        gemini_to_claude_list(
            paged_gemini,
            &one("models/gemini-1", gemini_claude_supplement()),
            &complete
        )
        .is_ok()
    );
}

#[test]
fn list_supplements_are_per_model_and_missing_key_is_named() {
    let mut second = openai_model();
    second.id = "openai-2".into();
    let body = {
        let mut fixture = openai_models::ListModelsResponseBody::builder(
            vec![openai_model(), second],
            openai_models::ListObject::List,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let error = openai_to_gemini_list(
        body,
        &one("openai-1", gemini_supplement()),
        &ListPageFacts::complete(),
    )
    .unwrap_err();
    assert!(error.context().contains("openai-2"));

    let mut second = openai_model();
    second.id = "openai-2".into();
    let body = {
        let mut fixture = openai_models::ListModelsResponseBody::builder(
            vec![openai_model(), second],
            openai_models::ListObject::List,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let mut supplements = one("openai-1", gemini_supplement());
    supplements.insert(
        "openai-2".into(),
        GeminiModelSupplement {
            base_model_id: "base-two".into(),
            version: "v2".into(),
        },
    );
    let converted = openai_to_gemini_list(body, &supplements, &ListPageFacts::complete())
        .unwrap()
        .value;
    assert_eq!(
        converted.models.as_ref().unwrap()[0].base_model_id,
        "base-supplied"
    );
    assert_eq!(
        converted.models.as_ref().unwrap()[1].base_model_id,
        "base-two"
    );
}

#[test]
fn paginated_targets_use_their_own_continuation_facts() {
    let body = {
        let mut fixture = gemini_models::ListModelsResponseBody::builder().build();
        fixture.models = Some(vec![gemini_model()]);
        fixture.next_page_token = Some("source-token".into());
        fixture.rest = Rest::new();
        fixture
    };
    let output = gemini_to_claude_list(
        body.clone(),
        &one("models/gemini-1", gemini_claude_supplement()),
        &ListPageFacts::explicit("gemini-1", "gemini-1", true),
    )
    .unwrap()
    .value;
    assert!(output.has_more);
    let mut terminal = body;
    terminal.next_page_token = None;
    assert!(
        gemini_to_claude_list(
            terminal.clone(),
            &one("models/gemini-1", gemini_claude_supplement()),
            &ListPageFacts::explicit("gemini-1", "gemini-1", false),
        )
        .is_ok()
    );
    assert!(
        gemini_to_openai_list(
            terminal,
            &one("models/gemini-1", openai_supplement()),
            &ListPageFacts::explicit("gemini-1", "gemini-1", false),
        )
        .is_ok()
    );
}

#[test]
fn complete_facts_cannot_claim_continuation_or_colliding_identities() {
    let body = {
        let mut fixture = openai_models::ListModelsResponseBody::builder(
            vec![openai_model()],
            openai_models::ListObject::List,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let mut contradictory = ListPageFacts::complete();
    contradictory.has_more = Some(true);
    assert!(
        openai_to_claude_list(
            body.clone(),
            &one("openai-1", claude_supplement()),
            &contradictory
        )
        .is_ok()
    );
    let contradictory = ListPageFacts::complete().with_next_page_token("next");
    assert!(
        openai_to_gemini_list(body, &one("openai-1", gemini_supplement()), &contradictory).is_ok()
    );
    let mut prefixed = openai_model();
    prefixed.id = "models/openai-1".into();
    let body = {
        let mut fixture = openai_models::ListModelsResponseBody::builder(
            vec![openai_model(), prefixed],
            openai_models::ListObject::List,
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let mut supplements = one("openai-1", gemini_supplement());
    supplements.insert("models/openai-1".into(), gemini_supplement());
    assert!(openai_to_gemini_list(body, &supplements, &ListPageFacts::complete()).is_ok());
}

#[test]
fn gemini_to_claude_requires_valid_supplied_timestamp() {
    let mut facts = gemini_claude_supplement();
    facts.created_at = Some("invalid".into());
    assert!(gemini_to_claude(gemini_model(), &facts).is_err());
    let output = claude_to_gemini(claude_model(), &gemini_supplement()).unwrap();
    assert!(
        output
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.field == "model.capabilities.thinking.types")
    );
}
