use super::*;
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
fn image(id: &str) -> Value {
    json!({"type":"image_generation_call","id":id,"result":PNG,"status":"completed"})
}
#[test]
fn inline_image_emits_one_complete_item_before_source_eof_and_preserves_order() {
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    let mut chunks = stream.push(gpart(json!([{"text":"before"},{"inlineData":{"mimeType":"image/png","data":PNG},"thoughtSignature":"actual-sig","foreign":"DROP"},{"text":"after"}]))).unwrap().value;
    assert_eq!(chunks.iter().filter(|event| matches!(event, s::StreamEvent::OutputItemDone(v) if matches!(v.item, r::ResponseOutputItem::ImageGenerationCall(_)))).count(), 1);
    assert!(
        !chunks
            .iter()
            .any(|event| matches!(event, s::StreamEvent::Completed(_)))
    );
    chunks.extend(stream.push(gfinish("STOP")).unwrap().value);
    chunks.extend(stream.finish().unwrap().chunks);
    let complete = collect_r(chunks);
    assert!(matches!(
        &complete.output[0],
        r::ResponseOutputItem::Message(_)
    ));
    let r::ResponseOutputItem::ImageGenerationCall(image) = &complete.output[1] else {
        panic!()
    };
    assert_eq!(image.result.as_deref(), Some(PNG));
    assert!(matches!(
        &complete.output[2],
        r::ResponseOutputItem::Message(_)
    ));
    assert!(
        !serde_json::to_string(&complete)
            .unwrap()
            .contains("actual-sig")
    );
}
#[test]
fn responses_image_done_emits_real_blob_once_before_overall_terminal() {
    let body = response(json!([image("ig-source")]), "completed", None);
    let mut stream =
        ResponsesToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    let mut out = Vec::new();
    let mut image_before_terminal = false;
    for event in r_events(body) {
        let done = matches!(event, s::StreamEvent::OutputItemDone(_));
        let chunks = stream.push(event).unwrap().value;
        if done {
            image_before_terminal = chunks
                .iter()
                .flat_map(|v| v.candidates.iter().flatten())
                .filter_map(|c| c.content.as_ref())
                .flat_map(|c| c.parts.iter().flatten())
                .any(|p| p.inline_data.is_some());
        }
        out.extend(chunks);
    }
    assert!(image_before_terminal);
    out.extend(stream.finish().unwrap().chunks);
    let complete = collect_g(out);
    let parts = complete.candidates.unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(parts.len(), 1);
    let blob = parts[0].inline_data.as_ref().unwrap();
    assert_eq!(blob.mime_type, "image/png");
    assert_eq!(blob.data, PNG);
}
#[test]
fn image_only_filters_text_in_buffered_and_incremental_outputs_without_faking_an_image() {
    let body = response(
        json!([message(json!([text("unrequested")])), image("ig")]),
        "completed",
        None,
    );
    let expected = pair::responses_to_gemini_response_with_modalities(
        body.clone(),
        Default::default(),
        Some(&[g::Modality::Image]),
    )
    .unwrap()
    .value;
    let mut stream = ResponsesToGeminiStream::new(
        ResponsesToGeminiContext {
            response_modalities: Some(vec![g::Modality::Image]),
            ..Default::default()
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut out = Vec::new();
    for event in r_events(body) {
        out.extend(stream.push(event).unwrap().value);
    }
    out.extend(stream.finish().unwrap().chunks);
    let actual = collect_g(out);
    assert_eq!(actual.candidates, expected.candidates);
    assert!(
        !serde_json::to_string(&actual)
            .unwrap()
            .contains("unrequested")
    );
    let no_image = response(
        json!([message(json!([text("only text")]))]),
        "completed",
        None,
    );
    assert!(
        pair::responses_to_gemini_response_with_modalities(
            no_image,
            Default::default(),
            Some(&[g::Modality::Image])
        )
        .is_err()
    );
}
#[test]
fn image_only_preserves_native_refusal_and_incomplete_reasons_without_fake_images() {
    for (status, reason, output, expected) in [
        (
            "completed",
            None,
            json!([message(
                json!([{"type":"refusal","refusal":"cannot generate"}])
            )]),
            g::FinishReason::Safety,
        ),
        (
            "incomplete",
            Some("content_filter"),
            json!([]),
            g::FinishReason::Safety,
        ),
        (
            "incomplete",
            Some("max_output_tokens"),
            json!([{ "type":"image_generation_call", "id":"ig_pending", "status":"failed", "result":null }]),
            g::FinishReason::MaxTokens,
        ),
    ] {
        let body = response(output, status, reason);
        let complete = pair::responses_to_gemini_response_with_modalities(
            body.clone(),
            Default::default(),
            Some(&[g::Modality::Image]),
        )
        .unwrap()
        .value;
        let candidate = &complete.candidates.as_ref().unwrap()[0];
        assert_eq!(candidate.finish_reason, Some(expected));
        assert!(
            candidate
                .content
                .as_ref()
                .unwrap()
                .parts
                .as_ref()
                .unwrap()
                .is_empty()
        );
        let mut stream = ResponsesToGeminiStream::new(
            ResponsesToGeminiContext {
                response_modalities: Some(vec![g::Modality::Image]),
                ..Default::default()
            },
            flow(),
            Default::default(),
        )
        .unwrap();
        let mut out = Vec::new();
        for event in r_events(body) {
            out.extend(stream.push(event).unwrap().value);
        }
        out.extend(stream.finish().unwrap().chunks);
        assert_eq!(collect_g(out).candidates, complete.candidates);
    }
}

#[test]
fn image_payload_validation_rejects_mime_mismatch_null_and_incomplete_results() {
    for invalid in [
        json!({"type":"image_generation_call","id":"ig","result":null,"status":"completed"}),
        json!({"type":"image_generation_call","id":"ig","result":PNG,"status":"generating"}),
    ] {
        assert!(
            pair::responses_to_gemini_response(
                response(json!([invalid]), "completed", None),
                Default::default()
            )
            .is_err()
        );
    }
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    assert!(
        stream
            .push(gpart(
                json!([{"inlineData":{"mimeType":"image/jpeg","data":PNG}}])
            ))
            .is_ok()
    );
}
#[test]
fn image_request_selectors_and_exact_format_controls_are_not_conflated() {
    let g_request = |modalities: Value| {
        serde_json::from_value::<g::GenerateContentRequestBody>(json!({"contents":[{"parts":[{"text":"draw"}]}],"generationConfig":{"responseModalities":modalities,"responseFormat":{"image":{"mimeType":"IMAGE_JPEG","delivery":"INLINE"}}}})).unwrap()
    };
    let out = pair::gemini_to_responses_request(
        g_request(json!(["IMAGE"])),
        "selected",
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let wire = serde_json::to_value(&out).unwrap();
    assert_eq!(wire["tool_choice"]["type"], "image_generation");
    assert_eq!(wire["tools"][0]["output_format"], "jpeg");
    let back = pair::responses_to_gemini_request(out, "gemini", Default::default())
        .unwrap()
        .value;
    assert_eq!(
        back.generation_config.unwrap().response_modalities,
        Some(vec![g::Modality::Image])
    );
    let disabled = serde_json::from_value(json!({"input":"text","tools":[{"type":"image_generation","size":"999x999"}],"tool_choice":"none"})).unwrap();
    let mapped = pair::responses_to_gemini_request(disabled, "gemini", Default::default())
        .unwrap()
        .value;
    assert_eq!(
        mapped.generation_config.unwrap().response_modalities,
        Some(vec![g::Modality::Text])
    );
    let required = serde_json::from_value(json!({"input":"x","tools":[{"type":"image_generation"},{"type":"function","name":"f","parameters":{},"strict":false}],"tool_choice":"required"})).unwrap();
    assert!(pair::responses_to_gemini_request(required, "gemini", Default::default()).is_ok());
    let mut approximate = g_request(json!(["TEXT", "IMAGE"]));
    approximate.generation_config.as_mut().unwrap().image_config =
        Some(g::ImageConfig::builder().image_size("1K").build());
    assert!(
        pair::gemini_to_responses_request(
            approximate,
            "selected",
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi)
        )
        .is_ok()
    );
}
#[test]
fn signed_inline_image_history_restores_only_original_model_origin_field_and_bytes() {
    fn replay() -> GeminiReplayContext {
        let target = IdentityTarget::new("actual-model", Dialect::Gemini)
            .unwrap()
            .with_origin("image-origin")
            .unwrap();
        let mut state = IdentityStateRecord::new(
            IdentityRole::OutputItem(OutputItemKind::ImageGenerationCall),
            target.clone(),
        );
        state.client_item_id = Some("ig-history".into());
        state.opaque_signature = Some(
            OpaqueSignature::new(
                OpaqueField::GeminiPartThoughtSignature,
                "opaque-image",
                "image-origin",
                "actual-model",
            )
            .unwrap(),
        );
        GeminiReplayContext {target:Some(target),parts:std::collections::BTreeMap::from([("ig-history".into(),RestoredGeminiPart{state,part:serde_json::from_value(json!({"inlineData":{"mimeType":"image/png","data":PNG},"thoughtSignature":"opaque-image","foreign":"DROP"})).unwrap()})]),image_files:Default::default()}
    }
    let request =
        || serde_json::from_value(json!({"model":"r","input":[image("ig-history")]})).unwrap();
    let restored = pair::responses_to_gemini_request(request(), "actual-model", replay())
        .unwrap()
        .value;
    let part = &restored.contents[0].parts.as_ref().unwrap()[0];
    assert_eq!(part.thought_signature.as_deref(), Some("opaque-image"));
    assert_eq!(part.inline_data.as_ref().unwrap().data, PNG);
    assert!(!serde_json::to_string(&restored).unwrap().contains("DROP"));
    assert!(pair::responses_to_gemini_request(request(), "different-model", replay()).is_err());
    let mut wrong_origin = replay();
    wrong_origin.target.as_mut().unwrap().origin = Some("other-origin".into());
    assert!(pair::responses_to_gemini_request(request(), "actual-model", wrong_origin).is_err());
    let mut changed = replay();
    changed
        .parts
        .get_mut("ig-history")
        .unwrap()
        .part
        .inline_data
        .as_mut()
        .unwrap()
        .data = "different-payload".into();
    assert!(pair::responses_to_gemini_request(request(), "actual-model", changed).is_err());
}
