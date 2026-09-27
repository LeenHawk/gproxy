use super::*;
#[test]
fn response_text_refusal_reasoning_and_functions_are_live_and_canonical() {
    let body = response(
        json!([message(json!([text("AB"),text("second"),{"type":"refusal","refusal":"denied"}])),{"type":"reasoning","id":"rs-source","summary":[{"type":"summary_text","text":"summary fallback"}],"status":"completed"},function("fc-one","call-one","{\"x\":1}"),function("fc-two","call-two","{}")]),
        "completed",
        None,
    );
    let mut split = Vec::new();
    for event in r_events(body) {
        match event {
            s::StreamEvent::OutputTextDelta(mut v) if v.delta == "AB" => {
                v.delta = "A".into();
                split.push(s::StreamEvent::OutputTextDelta(v.clone()));
                v.delta = "B".into();
                split.push(s::StreamEvent::OutputTextDelta(v));
            }
            _ => split.push(event),
        }
    }
    let events = renumber(split);
    let mut stream =
        ResponsesToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    let mut live_text = false;
    let mut live_tool = false;
    for event in events.clone() {
        let before_terminal = !matches!(
            event,
            s::StreamEvent::Completed(_) | s::StreamEvent::Incomplete(_)
        );
        for chunk in stream.push(event).unwrap().value {
            if before_terminal {
                for p in chunk
                    .candidates
                    .iter()
                    .flatten()
                    .filter_map(|c| c.content.as_ref())
                    .flat_map(|c| c.parts.iter().flatten())
                {
                    live_text |= p.text.as_deref() == Some("A");
                    live_tool |= p.function_call.is_some();
                }
            }
            assert!(
                chunk
                    .candidates
                    .iter()
                    .flatten()
                    .all(|c| c.finish_reason.is_none())
            );
        }
    }
    assert!(live_text && live_tool);
    stream.finish().unwrap();
    let actual = run_r(events);
    assert_eq!(
        actual.candidates.unwrap()[0].finish_reason,
        Some(g::FinishReason::Safety)
    );
}
#[test]
fn summary_waits_until_content_preference_is_known_and_empty_shape_survives() {
    let body = response(
        json!([{"type":"reasoning","id":"rs-source","summary":[{"type":"summary_text","text":"not final text"}],"content":[{"type":"reasoning_text","text":"actual thinking"}],"status":"completed"},message(json!([text("")]))]),
        "completed",
        None,
    );
    let events = r_events(body);
    let mut stream =
        ResponsesToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    let mut observed = String::new();
    for event in events.clone() {
        for chunk in stream.push(event).unwrap().value {
            for p in chunk
                .candidates
                .iter()
                .flatten()
                .filter_map(|c| c.content.as_ref())
                .flat_map(|c| c.parts.iter().flatten())
            {
                observed.push_str(p.text.as_deref().unwrap_or(""));
            }
        }
    }
    stream.finish().unwrap();
    assert!(observed.contains("actual thinking"));
    assert!(!observed.contains("not final text"));
    run_r(events);
    let actual = run_r(r_events(response(
        json!([message(json!([]))]),
        "completed",
        None,
    )));
    assert_eq!(
        actual.candidates.unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()
            .len(),
        0
    );
}
#[test]
fn interleaved_same_name_functions_preserve_wire_order_and_can_finish_before_items() {
    let events = r_events(response(
        json!([
            function("fc-one", "call-one", "{\"n\":1}"),
            function("fc-two", "call-two", "{\"n\":2}")
        ]),
        "completed",
        None,
    ));
    let mut created = Vec::new();
    let mut starts = Vec::new();
    let mut first = Vec::new();
    let mut second = Vec::new();
    let mut tail = Vec::new();
    for event in events {
        match &event {
            s::StreamEvent::Created(_) => created.push(event),
            s::StreamEvent::OutputItemAdded(_) => starts.push(event),
            s::StreamEvent::Completed(_) => tail.push(event),
            _ => {
                let v = serde_json::to_value(&event).unwrap();
                if v["output_index"] == 0 {
                    first.push(event)
                } else {
                    second.push(event)
                }
            }
        }
    }
    let reordered = renumber(
        created
            .into_iter()
            .chain(starts)
            .chain(second)
            .chain(first)
            .chain(tail)
            .collect(),
    );
    let actual = run_r(reordered);
    let calls: Vec<_> = actual.candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()
        .iter()
        .filter_map(|p| p.function_call.as_ref())
        .collect();
    assert_eq!(calls[0].id.as_deref(), Some("call-one"));
    assert_eq!(calls[1].id.as_deref(), Some("call-two"));
}
#[test]
fn message_parts_gate_interleaved_text_instead_of_reordering_plain_gemini_parts() {
    let events = r_events(response(
        json!([message(json!([text("first"), text("second")]))]),
        "completed",
        None,
    ));
    let mut begin = Vec::new();
    let mut starts = Vec::new();
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut tail = Vec::new();
    for event in events {
        match &event {
            s::StreamEvent::Created(_) | s::StreamEvent::OutputItemAdded(_) => begin.push(event),
            s::StreamEvent::ContentPartAdded(_) => starts.push(event),
            s::StreamEvent::OutputItemDone(_) | s::StreamEvent::Completed(_) => tail.push(event),
            _ => {
                let v = serde_json::to_value(&event).unwrap();
                if v["content_index"] == 0 {
                    p0.push(event)
                } else {
                    p1.push(event)
                }
            }
        }
    }
    let reordered = renumber(
        begin
            .into_iter()
            .chain(starts)
            .chain(p1)
            .chain(p0)
            .chain(tail)
            .collect(),
    );
    let actual = run_r(reordered);
    let joined = actual.candidates.unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()
        .iter()
        .filter_map(|p| p.text.as_deref())
        .collect::<Vec<_>>()
        .join("");
    assert_eq!(joined, "firstsecond");
}
#[test]
fn responses_target_policy_rewrites_response_and_call_ids_with_source_association() {
    let mut body = response(
        json!([
            function("fc-one", "a.b", "{}"),
            function("fc-two", "a_b", "{}")
        ]),
        "completed",
        None,
    );
    body.id = "response.native".into();
    let policy = TargetIdPolicy::new(Dialect::Gemini).with_syntax(IdSyntax::AsciiIdentifier);
    let mut stream = ResponsesToGeminiStream::new_with_policy(
        Default::default(),
        flow(),
        policy.clone(),
        Default::default(),
    )
    .unwrap();
    let mut out = Vec::new();
    for event in r_events(body) {
        out.extend(stream.push(event).unwrap().value);
    }
    let end = stream.finish().unwrap();
    out.extend(end.chunks);
    let actual = collect_g(out);
    assert!(policy.accepts_source(actual.response_id.as_ref().unwrap()));
    for call in actual.candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()
        .iter()
        .filter_map(|p| p.function_call.as_ref())
    {
        let id = call.id.as_ref().unwrap();
        assert!(policy.accepts_source(id));
        assert!(matches!(
            end.identities
                .lookup_emitted_as(IdentityRole::ToolCall, id)
                .unwrap()
                .source_id(),
            Some("a.b" | "a_b")
        ));
    }
}
#[test]
fn response_incomplete_causes_survive_without_using_initial_usage_as_final() {
    for (reason, expected) in [
        ("max_output_tokens", g::FinishReason::MaxTokens),
        ("content_filter", g::FinishReason::Safety),
    ] {
        let mut body = response(
            json!([message(json!([text("partial")]))]),
            "incomplete",
            Some(reason),
        );
        if let r::ResponseOutputItem::Message(m) = &mut body.output[0] {
            m.status = i::OutputMessageStatus::Incomplete;
        }
        let actual = run_r(r_events(body));
        assert_eq!(actual.candidates.unwrap()[0].finish_reason, Some(expected));
    }
    let body = response(json!([message(json!([text("answer")]))]), "completed", None);
    let usage = body.usage.clone();
    let events = r_events(body).into_iter().map(|e| match e {
        s::StreamEvent::Created(mut v) => {
            v.response.usage = usage.clone();
            s::StreamEvent::Created(v)
        }
        s::StreamEvent::Completed(mut v) => {
            v.response.usage = None;
            s::StreamEvent::Completed(v)
        }
        other => other,
    });
    let mut stream =
        ResponsesToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    for event in events {
        stream.push(event).unwrap();
    }
    assert_eq!(
        stream.finish().unwrap_err().kind(),
        TransformErrorKind::MissingMetadata
    );
}

#[test]
fn populated_creation_progress_and_partial_arguments_seed_once() {
    let mut events = r_events(response(
        json!([message(json!([text("seed-tail")]))]),
        "completed",
        None,
    ));
    let mut seed: r::ResponseOutputItem =
        serde_json::from_value(message(json!([text("seed")]))).unwrap();
    if let r::ResponseOutputItem::Message(v) = &mut seed {
        v.status = i::OutputMessageStatus::InProgress;
    }
    let s::StreamEvent::Created(created) = &mut events[0] else {
        unreachable!()
    };
    created.response.output = vec![seed];
    let mut progress = created.response.clone();
    if let r::ResponseOutputItem::Message(v) = &mut progress.output[0]
        && let i::OutputContent::Text(v) = &mut v.content[0]
    {
        v.text = "seed-tail".into();
    }
    events.retain(|e| {
        !matches!(
            e,
            s::StreamEvent::OutputItemAdded(_) | s::StreamEvent::ContentPartAdded(_)
        )
    });
    let n = events
        .iter()
        .position(|e| matches!(e, s::StreamEvent::OutputTextDelta(_)))
        .unwrap();
    if let s::StreamEvent::OutputTextDelta(v) = &mut events[n] {
        v.delta = "-tail".into();
    }
    events.insert(
        n + 1,
        s::StreamEvent::InProgress(s::ResponseInProgress::builder(0, progress).build()),
    );
    run_r(renumber(events));
    let mut events = r_events(response(
        json!([function("fc-one", "call-one", "{\"x\":1}")]),
        "completed",
        None,
    ));
    for event in &mut events {
        match event {
            s::StreamEvent::OutputItemAdded(v) => {
                if let r::ResponseOutputItem::FunctionCall(v) = &mut v.item {
                    v.id = None;
                    v.arguments = "{\"x\":".into();
                }
            }
            s::StreamEvent::FunctionCallArgumentsDelta(v) => v.delta = "1}".into(),
            _ => {}
        }
    }
    let actual = run_r(events);
    let part = &actual.candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()[0];
    assert_eq!(
        part.function_call.as_ref().unwrap().id.as_deref(),
        Some("call-one")
    );
    assert_eq!(
        part.function_call.as_ref().unwrap().args.as_ref().unwrap()["x"],
        1
    );
}
#[test]
fn actual_citation_ranges_logprobs_and_usage_arrive_without_replaying_text() {
    let mut part = text("answer");
    part["annotations"] = json!([{"type":"url_citation","start_index":0,"end_index":6,"url":"https://source.example","title":"actual source","foreign":"extension_marker"}]);
    part["logprobs"] = json!([{"token":"answer","logprob":-0.25,"bytes":[97,110,115,119,101,114],"top_logprobs":[{"token":"other","logprob":-1.0,"bytes":[111,116,104,101,114]}]}]);
    let actual = run_r(r_events(response(
        json!([message(json!([part]))]),
        "completed",
        None,
    )));
    let c = &actual.candidates.as_ref().unwrap()[0];
    let citation = &c
        .citation_metadata
        .as_ref()
        .unwrap()
        .citation_sources
        .as_ref()
        .unwrap()[0];
    assert_eq!(citation.start_index, Some(0));
    assert_eq!(citation.end_index, Some(6));
    assert_eq!(citation.uri.as_deref(), Some("https://source.example"));
    assert_eq!(
        c.logprobs_result
            .as_ref()
            .unwrap()
            .chosen_candidates
            .as_ref()
            .unwrap()[0]
            .log_probability,
        Some(-0.25)
    );
    assert!(
        !serde_json::to_string(&actual)
            .unwrap()
            .contains("extension_marker")
    );
}

#[test]
fn unsigned_empty_reasoning_is_omitted_and_reasoning_never_gains_a_signature() {
    run_r(r_events(response(
        json!([{"type":"reasoning","id":"rs-source","summary":[],"content":[{"type":"reasoning_text","text":""}],"status":"completed"}]),
        "completed",
        None,
    )));
    // A Responses upstream's reasoning carries no Gemini signature: its
    // text replays unsigned, whatever encrypted_content it holds.
    let body = response(
        json!([{"type":"reasoning","id":"rs-source","summary":[{"type":"summary_text","text":"summary"}],"content":[{"type":"reasoning_text","text":"actual thinking"}],"encrypted_content":"gemini:original-signature","status":"completed"}]),
        "completed",
        None,
    );
    let actual = run_r(r_events(body));
    let part = &actual.candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()[0];
    assert_eq!(part.text.as_deref(), Some("actual thinking"));
    assert_eq!(part.thought_signature, None);
}
