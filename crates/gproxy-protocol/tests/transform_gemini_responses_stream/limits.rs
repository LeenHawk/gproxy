use super::*;
#[test]
fn wrong_policy_and_context_bounds_fail_before_output_allocation() {
    assert!(
        GeminiToResponsesStream::new_with_policy(
            gc(),
            flow(),
            TargetIdPolicy::new(Dialect::Gemini),
            Default::default()
        )
        .is_err()
    );
    assert!(
        ResponsesToGeminiStream::new_with_policy(
            Default::default(),
            flow(),
            TargetIdPolicy::new(Dialect::OpenAi),
            Default::default()
        )
        .is_err()
    );
    let mut ctx = gc();
    ctx.actual_model = Some("x".repeat(1000));
    assert_eq!(
        GeminiToResponsesStream::new(
            ctx,
            flow(),
            StreamLimits {
                max_bytes: 100,
                ..Default::default()
            }
        )
        .err()
        .unwrap()
        .kind(),
        TransformErrorKind::Limit
    );
}
#[test]
fn limits_cover_pending_output_amplification_and_terminal_events() {
    let mut stream = GeminiToResponsesStream::new(
        gc(),
        flow(),
        StreamLimits {
            max_pending: 8,
            ..Default::default()
        },
    )
    .unwrap();
    let error = stream
        .push(ge(
            json!({"candidates":[{"content":{"parts":[{"text":"pending"}]}}]}),
        ))
        .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Limit);
    assert!(stream.push(gfinish("STOP")).is_err());
    let mut stream = GeminiToResponsesStream::new(
        gc(),
        flow(),
        StreamLimits {
            max_events: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        stream
            .push(gpart(json!([{"text":"amplify"}])))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Limit
    );
    let body = response(
        json!([function("fc", "call", "{\"argument\":\"large value\"}")]),
        "completed",
        None,
    );
    let mut stream = ResponsesToGeminiStream::new(
        Default::default(),
        flow(),
        StreamLimits {
            max_pending: 8,
            ..Default::default()
        },
    )
    .unwrap();
    let mut failed = false;
    for event in r_events(body) {
        if let Err(e) = stream.push(event) {
            assert_eq!(e.kind(), TransformErrorKind::Limit);
            failed = true;
            break;
        }
    }
    assert!(failed);
}
#[test]
fn errors_unsupported_native_payloads_and_premature_eof_poison_streams() {
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    assert!(
        stream
            .push(gpart(
                json!([{"inlineData":{"mimeType":"image/png","data":"AQI="}}])
            ))
            .is_err()
    );
    assert!(stream.finish().is_err());
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    stream.push(gpart(json!([{"text":"partial"}]))).unwrap();
    assert!(stream.finish().is_err());
    let mut stream =
        ResponsesToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    let mut events = r_events(response(
        json!([message(json!([text("ok")]))]),
        "completed",
        None,
    ));
    events.pop();
    for event in events {
        stream.push(event).unwrap();
    }
    assert!(stream.finish().is_err());
}

#[test]
fn actual_provider_error_and_contradictory_model_never_produce_success_tail() {
    let mut stream =
        ResponsesToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    let created = r_events(response(json!([]), "completed", None)).remove(0);
    stream.push(created).unwrap();
    let error: s::StreamEvent=serde_json::from_value(json!({"type":"error","sequence_number":1,"code":"provider_limit","message":"actual provider detail","param":"model"})).unwrap();
    let error = stream.push(error).unwrap_err();
    assert!(error.to_string().contains("actual provider detail"));
    assert!(error.to_string().contains("provider_limit"));
    assert!(stream.finish().is_err());
    let mut ctx = gc();
    ctx.actual_model = Some("bound-model".into());
    let mut stream = GeminiToResponsesStream::new(ctx, flow(), Default::default()).unwrap();
    assert!(stream.push(gpart(json!([{"text":"wrong model"}]))).is_err());
    assert!(stream.finish().is_err());
}
