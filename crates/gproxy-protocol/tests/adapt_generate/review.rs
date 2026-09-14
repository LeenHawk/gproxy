use super::*;
use gproxy_protocol::transform::identity::IdSyntax;
fn strict(client: Dialect, upstream: Dialect) -> GenerationIdentity {
    let mut value = ids(client, upstream);
    value.request_policy = value.request_policy.with_syntax(IdSyntax::AsciiIdentifier);
    value
}
fn response_history() -> gproxy_protocol::wire::openai::responses::GenerateContentRequestBody {
    let mut value = input("r");
    value["input"] = json!([
        {"type":"function_call","call_id":"a.b","name":"lookup","arguments":"{}"},
        {"type":"function_call_output","call_id":"a.b","output":"ok","name":"lookup"}
    ]);
    serde_json::from_value(value).unwrap()
}
#[test]
fn review_responses_to_chat_request_applies_selected_policy() {
    let identity = strict(Dialect::OpenAi, Dialect::OpenAiChat);
    let p = ResponsesViaChat::prepare(response_history(), "selected", endpoint(), identity.clone())
        .unwrap();
    let v = serde_json::to_value(p.target_request()).unwrap();
    let id = v["messages"][0]["tool_calls"][0]["id"].as_str().unwrap();
    assert!(
        identity.request_policy.accepts_source(id),
        "policy violated by {id}"
    );
}
#[test]
fn review_responses_to_claude_request_applies_selected_policy() {
    let identity = strict(Dialect::OpenAi, Dialect::Claude);
    let p = ResponsesViaClaude::prepare(
        response_history(),
        "selected",
        endpoint(),
        identity.clone(),
        Default::default(),
    )
    .unwrap();
    let v = serde_json::to_value(p.target_request()).unwrap();
    let id = v["messages"][0]["content"][0]["id"].as_str().unwrap();
    assert!(
        identity.request_policy.accepts_source(id),
        "policy violated by {id}"
    );
}
#[test]
fn review_responses_to_gemini_request_applies_selected_policy() {
    let identity = strict(Dialect::OpenAi, Dialect::Gemini);
    let p = ResponsesViaGemini::prepare(
        response_history(),
        "selected",
        endpoint(),
        identity.clone(),
        Default::default(),
    )
    .unwrap();
    let v = serde_json::to_value(p.target_request()).unwrap();
    let id = v["contents"][0]["parts"][0]["functionCall"]["id"]
        .as_str()
        .unwrap();
    assert!(
        identity.request_policy.accepts_source(id),
        "policy violated by {id}"
    );
}
#[test]
fn review_chat_to_gemini_request_applies_selected_policy() {
    let mut value = input("h");
    value["messages"] = json!([
        {"role":"assistant","tool_calls":[{"type":"function","id":"a.b","function":{"name":"lookup","arguments":"{}"}}]},
        {"role":"tool","tool_call_id":"a.b","content":"ok"}
    ]);
    let identity = strict(Dialect::OpenAiChat, Dialect::Gemini);
    let p = ChatViaGemini::prepare(
        serde_json::from_value(value).unwrap(),
        "selected",
        endpoint(),
        identity.clone(),
        &Default::default(),
    )
    .unwrap();
    let v = serde_json::to_value(p.target_request()).unwrap();
    let id = v["contents"][0]["parts"][0]["functionCall"]["id"]
        .as_str()
        .unwrap();
    assert!(
        identity.request_policy.accepts_source(id),
        "policy violated by {id}"
    );
}
#[test]
fn review_chat_to_gemini_response_applies_selected_policy() {
    let mut identity = ids(Dialect::Gemini, Dialect::OpenAiChat);
    identity.response_policy = identity
        .response_policy
        .with_syntax(IdSyntax::AsciiIdentifier);
    let mut p = GeminiViaChat::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        identity.clone(),
    )
    .unwrap();
    let mut body = output("h");
    body["id"] = json!("r.x");
    body["choices"][0]["message"]["tool_calls"] =
        json!([{"type":"function","id":"a.b","function":{"name":"lookup","arguments":"{}"}}]);
    body["choices"][0]["finish_reason"] = json!("tool_calls");
    let converted = p
        .convert_response(serde_json::from_value(body).unwrap(), ())
        .unwrap();
    let v = serde_json::to_value(converted.value).unwrap();
    assert!(
        identity
            .response_policy
            .accepts_source(v["responseId"].as_str().unwrap())
    );
    assert!(
        identity.response_policy.accepts_source(
            v["candidates"][0]["content"]["parts"][1]["functionCall"]["id"]
                .as_str()
                .unwrap()
        )
    );
}
#[test]
fn review_gemini_image_metadata_cannot_hide_invalid_bytes() {
    let mut access = Resources::png();
    access.bytes = bytes::Bytes::from_static(b"not an image");
    access.length = Some(access.bytes.len() as u64);
    let scope = "source".to_string();
    let ctx = resources(&access, &scope);
    let req = serde_json::from_value(json!({"contents":[{"role":"user","parts":[{"fileData":{"fileUri":"https://source/image","mimeType":"image/png"}}]}]})).unwrap();
    assert!(
        ready(ctx.gemini(req)).is_err(),
        "invalid image accepted as image/png"
    );
}
#[test]
fn review_legacy_chat_response_gets_saved_generated_gemini_identity() {
    let mut p = GeminiViaChat::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
    )
    .unwrap();
    let mut body = output("h");
    body["choices"][0]["message"]["function_call"] =
        json!({"name":"legacy_lookup","arguments":"{}"});
    body["choices"][0]["finish_reason"] = json!("function_call");
    let host = Host::new(body);
    let store = Store::default();
    let state = state(&store, Dialect::OpenAiChat);
    let mut progress = GenerationProgress::default();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        |_| Ok(()),
    ))
    .unwrap() else {
        panic!("rejected")
    };
    let id = response.body.candidates.unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()[1]
        .function_call
        .as_ref()
        .unwrap()
        .id
        .clone()
        .unwrap();
    let replay =
        ready(state.recover_tools(std::slice::from_ref(&id), &Default::default())).unwrap();
    assert_eq!(replay.names[&id], "legacy_lookup");
    assert!(replay.original_call_ids.is_empty());
}
#[test]
fn review_recovery_binds_both_target_id_policies() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::new(output("c"));
    let mut first = chat_via_claude();
    let mut progress = GenerationProgress::default();
    ready(
        first.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Err(TransformError::missing_metadata("clock"))
        }),
    )
    .unwrap_err();
    for request in [false, true] {
        let mut identity = ids(Dialect::OpenAiChat, Dialect::Claude);
        if request {
            identity.request_policy = identity
                .request_policy
                .with_syntax(IdSyntax::AsciiIdentifier);
        } else {
            identity.response_policy = identity
                .response_policy
                .with_syntax(IdSyntax::AsciiIdentifier);
        }
        let mut changed = ChatViaClaude::prepare(
            serde_json::from_value(input("h")).unwrap(),
            "selected",
            endpoint(),
            identity,
        )
        .unwrap();
        let error = ready(changed.recover(codec_limits(), &state, &mut progress, |_| {
            Ok(claude_chat::ResponseSupplement {
                created_unix_seconds: Some(123),
            })
        }))
        .unwrap_err();
        assert_eq!(error.kind(), TransformErrorKind::Conflict);
    }
    assert!(store.entries.lock().unwrap().is_empty());
}

fn responses_call_host() -> Host {
    let mut body = output("r");
    body["id"] = json!("response.source");
    body["output"] = json!([{"type":"function_call","id":"fc_native","call_id":"a.b","name":"lookup","arguments":"{}","status":"completed"}]);
    Host::new(body)
}
fn responses_gemini_strict() -> GeminiViaResponses {
    let mut identity = ids(Dialect::Gemini, Dialect::OpenAi);
    identity.response_policy = identity
        .response_policy
        .with_syntax(IdSyntax::AsciiIdentifier);
    GeminiViaResponses::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        identity,
    )
    .unwrap()
}
#[test]
fn review_responses_to_gemini_policy_saves_actual_source_call_id() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAi);
    let host = responses_call_host();
    let mut p = responses_gemini_strict();
    let mut progress = GenerationProgress::default();
    let GenerationOutcome::Success { response, .. } =
        ready(
            p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
                Ok(Default::default())
            }),
        )
        .unwrap()
    else {
        panic!("rejected")
    };
    let policy = &p.identities().response_policy;
    assert!(policy.accepts_source(response.body.response_id.as_ref().unwrap()));
    let call_id = response.body.candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()[0]
        .function_call
        .as_ref()
        .unwrap()
        .id
        .as_ref()
        .unwrap();
    assert!(policy.accepts_source(call_id));
    let replay =
        ready(state.recover_tools(std::slice::from_ref(call_id), &Default::default())).unwrap();
    assert_eq!(replay.original_call_ids[call_id], "a.b");
    assert_eq!(replay.names[call_id], "lookup");
}
fn signed_gemini_replay(
    id: Option<&str>,
) -> gproxy_protocol::transform::generate::gemini_responses::GeminiReplayContext {
    use gproxy_protocol::transform::{
        generate::gemini_responses::{GeminiReplayContext, RestoredGeminiPart},
        identity::{
            IdentityRole, IdentityStateRecord, IdentityTarget, OpaqueField, OpaqueSignature,
        },
    };
    let target = IdentityTarget::new("selected", Dialect::Gemini)
        .unwrap()
        .with_origin("original-gemini")
        .unwrap();
    let mut state = IdentityStateRecord::new(IdentityRole::ToolCall, target.clone());
    state.client_call_id = Some("a.b".into());
    state.original_call_id = id.map(str::to_owned);
    state.tool_name = Some("lookup".into());
    state.opaque_signature = Some(
        OpaqueSignature::new(
            OpaqueField::GeminiPartThoughtSignature,
            "actual-signature",
            "original-gemini",
            "selected",
        )
        .unwrap(),
    );
    let mut part =
        json!({"thoughtSignature":"actual-signature","functionCall":{"name":"lookup","args":{}}});
    if let Some(id) = id {
        part["functionCall"]["id"] = json!(id);
    }
    GeminiReplayContext {
        target: Some(target),
        parts: std::collections::BTreeMap::from([(
            "a.b".into(),
            RestoredGeminiPart {
                state,
                part: serde_json::from_value(part).unwrap(),
            },
        )]),
    }
}
#[test]
fn review_restored_signed_gemini_id_or_absence_is_immutable_and_associated() {
    for original_id in [Some("original_native"), None] {
        let store = Store::default();
        let state = state(&store, Dialect::OpenAi);
        let host = responses_call_host();
        let mut p = responses_gemini_strict();
        let mut progress = GenerationProgress::default();
        let GenerationOutcome::Success { response, .. } =
            ready(
                p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
                    Ok(signed_gemini_replay(original_id))
                }),
            )
            .unwrap()
        else {
            panic!("rejected")
        };
        let part = &response.body.candidates.as_ref().unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()[0];
        assert_eq!(part.thought_signature.as_deref(), Some("actual-signature"));
        assert_eq!(
            part.function_call.as_ref().unwrap().id.as_deref(),
            original_id
        );
        if let Some(id) = original_id {
            let replay = ready(state.recover_tools(&[id.into()], &Default::default())).unwrap();
            assert_eq!(replay.original_call_ids[id], "a.b");
        } else {
            assert_eq!(store.entries.lock().unwrap().len(), 1);
        }
    }
}
#[test]
fn review_restored_signed_gemini_id_conflicting_with_policy_rejects() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAi);
    let host = responses_call_host();
    let mut p = responses_gemini_strict();
    let mut progress = GenerationProgress::default();
    let error = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(signed_gemini_replay(Some("original.native")))
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Unsupported);
    assert!(store.entries.lock().unwrap().is_empty());
    assert!(progress.raw_response.is_some());
}

#[test]
fn review_signed_replay_cannot_hide_duplicate_actual_native_call_ids() {
    let mut body = output("r");
    let call = json!({"type":"function_call","id":"fc_native","call_id":"a.b","name":"lookup","arguments":"{}","status":"completed"});
    let mut second = call.clone();
    second["id"] = json!("fc_second");
    body["output"] = json!([call, second]);
    let host = Host::new(body);
    let store = Store::default();
    let state = state(&store, Dialect::OpenAi);
    let mut p = responses_gemini_strict();
    let mut progress = GenerationProgress::default();
    let error = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(signed_gemini_replay(Some("original_native")))
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidResult);
    assert!(
        error
            .to_string()
            .contains("duplicate actual native call ID")
    );
    assert!(store.entries.lock().unwrap().is_empty());
}
