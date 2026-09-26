use super::*;
#[test]
fn executes_shared_historical_fixtures_with_actual_capability_host() {
    let cases: Value = serde_json::from_str(include_str!(
        "../fixtures/generation_invocation_parity.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let store = Store::default();
        let upstream = if case["upstream"] == "claude" {
            Dialect::Claude
        } else {
            Dialect::Gemini
        };
        let state = state(&store, upstream);
        let mut reads = 0;
        let output = if case["operation"] == "response" {
            let host = Host::new(case["input"].clone());
            if upstream == Dialect::Claude {
                let mut p = chat_via_claude();
                let mut progress = GenerationProgress::default();
                let GenerationOutcome::Success { response, .. } =
                    ready(
                        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
                            Ok(claude_chat::ResponseSupplement {
                                created_unix_seconds: Some(123),
                            })
                        }),
                    )
                    .unwrap()
                else {
                    panic!("rejected")
                };
                serde_json::to_value(response.body).unwrap()
            } else {
                let mut p = chat_gemini();
                let mut progress = GenerationProgress::default();
                let GenerationOutcome::Success { response, .. } = ready(p.invoke(
                    &host,
                    &(),
                    codec_limits(),
                    &state,
                    &mut progress,
                    gemini_facts,
                ))
                .unwrap() else {
                    panic!("rejected")
                };
                let value = serde_json::to_value(response.body).unwrap();
                let calls = value["choices"][0]["message"]["tool_calls"]
                    .as_array()
                    .unwrap();
                assert_ne!(calls[0]["id"], calls[1]["id"]);
                assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
                value
            }
        } else {
            if let Some(prime) = case.get("prime") {
                let host = Host::new(prime.clone());
                let mut p = chat_gemini();
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
            }
            let access = Resources::png();
            let scope = "source-client".into();
            let resources = resources(&access, &scope);
            let prepared = ready(ChatViaGemini::prepare_with_capabilities(
                serde_json::from_value(case["input"].clone()).unwrap(),
                endpoint(),
                ids(Dialect::OpenAiChat, Dialect::Gemini),
                &state,
                &resources,
                &Default::default(),
            ));
            if case["name"] == "truncated_tool_result" {
                // The primed call kept Gemini's own ID, so nothing recorded its
                // name, and a Chat tool result without its call cannot supply
                // one. v3 rejected this orphan too.
                let Err(error) = prepared else {
                    panic!("orphan Gemini result without a name")
                };
                assert_eq!(error.kind(), TransformErrorKind::MissingState);
                assert!(store.entries.lock().unwrap().is_empty());
                println!(
                    "GENERATION_PARITY {}",
                    json!({"name":case["name"],"accepted":false,"error":error.to_string()})
                );
                continue;
            }
            let mut p = prepared.unwrap();
            let mut native = super::output("g");
            native["responseId"] = json!("actual-next-response");
            let host = Host::new(native);
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
            reads = access.reads.lock().unwrap().len();
            let sent = host.sent.lock().unwrap();
            assert_eq!(sent.len(), 1);
            let HttpBody::Bytes(bytes) = &sent[0].body else {
                panic!("not bytes")
            };
            let value: Value = serde_json::from_slice(bytes).unwrap();
            assert_eq!(reads, 1);
            assert_eq!(
                value["contents"][0]["parts"][0]["inlineData"]["mimeType"],
                "image/png"
            );
            value
        };
        println!(
            "GENERATION_PARITY {}",
            json!({"name":case["name"],"accepted":true,"output":output,"saved_records":store.entries.lock().unwrap().len(),"resource_reads":reads})
        );
    }
}
