use super::*;
#[test]
fn actual_gemini_parts_are_live_and_keep_independent_items_and_same_name_tools() {
    let first = gpart(json!([{"text":"first","foreign":"extension_marker"}]));
    let second = gpart(
        json!([{"text":"second"},{"text":"thinking","thought":true,"thoughtSignature":"original-signature"},{"functionCall":{"name":"same","args":{"arg_marker":1}}},{"functionCall":{"name":"same","args":{"arg_marker":2}},"thought":true}]),
    );
    let mut last = gfinish("STOP");
    last.response_id = Some("late-source-id".into());
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    let mut out = stream.push(first.clone()).unwrap().value;
    assert!(
        out.iter()
            .any(|v| matches!(v,s::StreamEvent::OutputTextDelta(v) if v.delta=="first"))
    );
    let created = out
        .iter()
        .find_map(|v| {
            if let s::StreamEvent::Created(v) = v {
                Some(&v.response)
            } else {
                None
            }
        })
        .unwrap();
    assert!(created.usage.is_none());
    assert_eq!(created.model, "actual-model");
    let early_id = created.id.clone();
    out.extend(stream.push(second.clone()).unwrap().value);
    out.extend(stream.push(last.clone()).unwrap().value);
    assert!(!out.iter().any(|v| matches!(
        v,
        s::StreamEvent::Completed(_) | s::StreamEvent::Incomplete(_)
    )));
    let end = stream.finish().unwrap();
    out.extend(end.chunks);
    let actual = collect_r(out);
    assert_eq!(actual.id, early_id);
    assert_eq!(actual.output.len(), 5);
    let calls: Vec<_> = actual
        .output
        .iter()
        .filter_map(|v| {
            if let r::ResponseOutputItem::FunctionCall(v) = v {
                Some(v)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].call_id, calls[1].call_id);
    assert_ne!(calls[0].id.as_deref(), Some(calls[0].call_id.as_str()));
    assert!(
        !serde_json::to_string(&actual)
            .unwrap()
            .contains("extension_marker")
    );
    assert!(
        serde_json::to_string(&actual)
            .unwrap()
            .contains("arg_marker")
    );
    let expected = pair::gemini_to_responses_response(
        collect_g(vec![first, second, last]),
        context(),
        &mut end.identities.clone(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    assert_eq!(actual, expected);
}
#[test]
fn gemini_stop_incomplete_empty_text_and_empty_output_are_canonical() {
    for (reason, status) in [
        ("STOP", r::ResponseStatus::Completed),
        ("MAX_TOKENS", r::ResponseStatus::Incomplete),
        ("SAFETY", r::ResponseStatus::Incomplete),
    ] {
        let actual = run_g(vec![
            gpart(json!([{"text":""},{"text":"thought","thought":true}])),
            gfinish(reason),
        ]);
        assert_eq!(actual.status, Some(status));
    }
    let actual = run_g(vec![gpart(json!([])), gfinish("STOP")]);
    assert!(actual.output.is_empty());
}
#[test]
fn missing_model_waits_boundedly_and_explicit_actual_model_is_not_request_alias() {
    let first = ge(json!({"candidates":[{"content":{"parts":[{"text":"early"}]}}]}));
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    assert!(stream.push(first.clone()).unwrap().value.is_empty());
    let mut last = gfinish("STOP");
    last.model_version = Some("actual-model".into());
    let released = stream.push(last).unwrap().value;
    assert!(
        released
            .iter()
            .any(|v| matches!(v,s::StreamEvent::OutputTextDelta(v) if v.delta=="early"))
    );
    stream.finish().unwrap();
    let mut ctx = gc();
    ctx.actual_model = Some("host-actual".into());
    let mut stream = GeminiToResponsesStream::new(ctx, flow(), Default::default()).unwrap();
    let mut out = stream.push(first).unwrap().value;
    out.extend(stream.push(gfinish("STOP")).unwrap().value);
    out.extend(stream.finish().unwrap().chunks);
    assert_eq!(collect_r(out).model, "host-actual");
}
#[test]
fn stale_initial_thinking_is_not_frozen_as_final_and_missing_final_usage_fails() {
    let mut first = gpart(json!([{"text":"early"}]));
    first.usage_metadata=Some(serde_json::from_value(json!({"promptTokenCount":9,"cachedContentTokenCount":4,"candidatesTokenCount":1,"thoughtsTokenCount":0,"totalTokenCount":10})).unwrap());
    let mut last = gfinish("STOP");
    last.usage_metadata.as_mut().unwrap().thoughts_token_count = None;
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    let mut out = stream.push(first.clone()).unwrap().value;
    out.extend(stream.push(last).unwrap().value);
    out.extend(stream.finish().unwrap().chunks);
    let actual = collect_r(out);
    assert_eq!(
        actual
            .usage
            .flatten()
            .unwrap()
            .output_tokens_details
            .reasoning_tokens,
        1
    );
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    stream.push(first).unwrap();
    stream.push(gpart(json!([{"text":"later"}]))).unwrap();
    stream
        .push(ge(json!({"candidates":[{"finishReason":"STOP"}]})))
        .unwrap();
    assert_eq!(
        stream.finish().unwrap_err().kind(),
        TransformErrorKind::MissingMetadata
    );
}
#[test]
fn final_thinking_fact_resolves_unknown_split_but_never_manufactures_prefix_zero() {
    let mut last = gfinish("STOP");
    let usage = last.usage_metadata.as_mut().unwrap();
    usage.candidates_token_count = None;
    usage.thoughts_token_count = None;
    let mut ctx = gc();
    ctx.final_thinking_tokens = Some(1);
    let mut stream = GeminiToResponsesStream::new(ctx, flow(), Default::default()).unwrap();
    let mut out = stream
        .push(gpart(json!([{"text":"answer"}])))
        .unwrap()
        .value;
    out.extend(stream.push(last.clone()).unwrap().value);
    out.extend(stream.finish().unwrap().chunks);
    let usage = collect_r(out).usage.flatten().unwrap();
    assert_eq!(usage.output_tokens, 5);
    assert_eq!(usage.output_tokens_details.reasoning_tokens, 1);
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    stream.push(gpart(json!([{"text":"answer"}]))).unwrap();
    stream.push(last).unwrap();
    assert_eq!(
        stream.finish().unwrap_err().kind(),
        TransformErrorKind::MissingMetadata
    );
}
#[test]
fn explicit_response_policy_controls_late_ids_and_function_aliases() {
    let policy = TargetIdPolicy::new(Dialect::OpenAi).with_syntax(IdSyntax::AsciiIdentifier);
    let mut stream =
        GeminiToResponsesStream::new_with_policy(gc(), flow(), policy.clone(), Default::default())
            .unwrap();
    let mut out=stream.push(gpart(json!([{"functionCall":{"id":"a.b","name":"same","args":{}}},{"functionCall":{"id":"a_b","name":"same","args":{}}}]))).unwrap().value;
    let mut last = gfinish("STOP");
    last.response_id = Some("response.native".into());
    out.extend(stream.push(last).unwrap().value);
    let end = stream.finish().unwrap();
    out.extend(end.chunks);
    let body = collect_r(out);
    assert!(policy.accepts_source(&body.id));
    for item in body.output {
        if let r::ResponseOutputItem::FunctionCall(call) = item {
            assert!(policy.accepts_source(&call.call_id));
            let handle = end
                .identities
                .lookup_emitted_as(IdentityRole::ToolCall, &call.call_id)
                .unwrap();
            assert!(matches!(handle.source_id(), Some("a.b" | "a_b")));
        }
    }
}
