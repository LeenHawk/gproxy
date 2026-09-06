use bytes::Bytes;

use super::memory::MemoryHost;
use super::{block_on, core, request, target};
use crate::control::{FailoverBudget, Plan};

#[test]
fn codex_model_list_uses_structured_metadata() -> Result<(), crate::InitError> {
    let host = MemoryHost::new(false);
    let mut state = host.state.lock().expect("state lock");
    state.plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let model = &mut state.exposed_models[0];
    model.context_window = Some(272_000);
    model.metadata.instructions = Some("model instructions".into());
    model.metadata.reasoning_levels = Some(vec![crate::ModelReasoningLevel {
        effort: "high".into(),
        description: "Deep reasoning".into(),
    }]);
    model.metadata.default_reasoning_level = Some("high".into());
    model.metadata.service_tiers = Some(vec![crate::ModelServiceTier {
        id: "priority".into(),
        name: "Fast".into(),
        description: "Faster responses".into(),
    }]);
    drop(state);
    let core = core(&host)?;
    let mut request = request(false, "codex-local-models");
    request.method = http::Method::GET;
    request.path = "/v1/models".into();
    request.body = Bytes::new();
    request.headers.insert(
        http::header::USER_AGENT,
        "codex_cli_rs/0.153.2".parse().unwrap(),
    );
    let outcome = block_on(core.execute(&host, request)).expect("Codex model list");
    let crate::ResponseBody::Full(body) = outcome.body else {
        panic!("Codex model list was not buffered");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(body.get("data").is_none());
    assert_eq!(body["models"][0]["slug"], "alias");
    assert_eq!(body["models"][0]["base_instructions"], "model instructions");
    assert_eq!(
        body["models"][0]["supported_reasoning_levels"][0]["effort"],
        "high"
    );
    assert_eq!(body["models"][0]["service_tiers"][0]["id"], "priority");
    Ok(())
}

#[test]
fn claude_and_gemini_lists_project_structured_metadata() -> Result<(), crate::InitError> {
    let host = MemoryHost::new(false);
    let mut state = host.state.lock().expect("state lock");
    state.plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let model = &mut state.exposed_models[0];
    model.metadata.batch_supported = Some(true);
    model.metadata.input_modalities = Some(vec!["text".into(), "image".into()]);
    model.metadata.generation_methods = Some(vec!["generateContent".into()]);
    drop(state);
    let core = core(&host)?;

    let mut claude = request(false, "claude-model-metadata");
    claude.method = http::Method::GET;
    claude.path = "/v1/models".into();
    claude.body = Bytes::new();
    claude
        .headers
        .insert("anthropic-version", "2023-06-01".parse().unwrap());
    let outcome = block_on(core.execute(&host, claude)).expect("Claude model list");
    let crate::ResponseBody::Full(body) = outcome.body else {
        panic!("Claude model list was not buffered");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["data"][0]["capabilities"]["batch"]["supported"], true);
    assert_eq!(
        body["data"][0]["capabilities"]["thinking"]["supported"],
        false
    );

    let mut gemini = request(false, "gemini-model-metadata");
    gemini.method = http::Method::GET;
    gemini.path = "/v1beta/models".into();
    gemini.body = Bytes::new();
    let outcome = block_on(core.execute(&host, gemini)).expect("Gemini model list");
    let crate::ResponseBody::Full(body) = outcome.body else {
        panic!("Gemini model list was not buffered");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["models"][0]["supportedGenerationMethods"][0],
        "generateContent"
    );
    Ok(())
}
