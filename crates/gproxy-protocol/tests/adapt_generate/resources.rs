use super::*;

#[test]
fn foreign_chat_image_is_read_in_source_scope_and_sent_as_actual_gemini_bytes() {
    let access = Resources::png();
    let scope = "source-openai-principal".into();
    let resources = resources(&access, &scope);
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let mut request = input("h");
    request["messages"] = json!([{"role":"user","content":[{"type":"image_url","image_url":{"url":"https://source/private.png","foreign":"ignore"}}]}]);
    let mut p = ready(ChatViaGemini::prepare_with_capabilities(
        serde_json::from_value(request).unwrap(),
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Gemini),
        &state,
        &resources,
        &Default::default(),
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    assert_eq!(
        target["contents"][0]["parts"][0]["inlineData"]["mimeType"],
        "image/png"
    );
    assert!(target["contents"][0]["parts"][0].get("fileData").is_none());
    assert_eq!(access.reads.lock().unwrap()[0].0, scope);
    let host = Host::new(output("g"));
    let mut progress = GenerationProgress::default();
    ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        gemini_facts,
    ))
    .unwrap();
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn three_other_source_dialects_materialize_declared_ids_and_uris() {
    let access = Resources::png();
    let scope = "source".into();
    let resources = resources(&access, &scope);
    let mut claude = input("c");
    claude["messages"] = json!([{"role":"user","content":[{"type":"image","source":{"type":"file","file_id":"actual-file"}}]}]);
    let claude = serde_json::to_value(
        ready(resources.claude(serde_json::from_value(claude).unwrap())).unwrap(),
    )
    .unwrap();
    assert_eq!(
        claude["messages"][0]["content"][0]["source"]["type"],
        "base64"
    );
    let mut gemini = input("g");
    gemini["contents"] = json!([{"role":"user","parts":[{"fileData":{"fileUri":"gs://actual/image","mimeType":"image/png"}}]}]);
    let gemini = serde_json::to_value(
        ready(resources.gemini(serde_json::from_value(gemini).unwrap())).unwrap(),
    )
    .unwrap();
    assert!(gemini["contents"][0]["parts"][0].get("fileData").is_none());
    assert_eq!(
        gemini["contents"][0]["parts"][0]["inlineData"]["mimeType"],
        "image/png"
    );
    let mut responses = input("r");
    responses["input"] = json!([{"role":"user","content":[{"type":"input_image","detail":"auto","file_id":"actual-file"}]}]);
    let responses = serde_json::to_value(
        ready(resources.responses(serde_json::from_value(responses).unwrap())).unwrap(),
    )
    .unwrap();
    assert!(
        responses["input"][0]["content"][0]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(responses["input"][0]["content"][0].get("file_id").is_none());
    assert_eq!(access.reads.lock().unwrap().len(), 3);
}

#[test]
fn resource_lengths_mime_and_aggregate_budgets_are_checked() {
    let mut access = Resources::png();
    access.length = Some(1);
    let scope = "source".into();
    let mut request = input("h");
    request["messages"] = json!([{"role":"user","content":[{"type":"image_url","image_url":{"url":"https://source/image"}}]}]);
    assert_eq!(
        ready(resources(&access, &scope).chat(serde_json::from_value(request.clone()).unwrap()))
            .unwrap_err()
            .kind(),
        TransformErrorKind::InvalidResult
    );
    access.length = Some(access.bytes.len() as u64);
    access.mime = "image/jpeg".into();
    assert!(
        ready(resources(&access, &scope).chat(serde_json::from_value(request.clone()).unwrap()))
            .is_err()
    );
    access.mime = "image/png".into();
    let part = request["messages"][0]["content"][0].clone();
    request["messages"][0]["content"] = json!([part.clone(), part]);
    let mut ctx = resources(&access, &scope);
    ctx.limits.max_body_bytes = access.bytes.len() as u64;
    assert_eq!(
        ready(ctx.chat(serde_json::from_value(request).unwrap()))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Limit
    );
}
