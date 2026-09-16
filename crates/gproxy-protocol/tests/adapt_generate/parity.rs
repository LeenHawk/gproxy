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
            let mut p = ready(ChatViaGemini::prepare_with_capabilities(
                serde_json::from_value(case["input"].clone()).unwrap(),
                endpoint(),
                ids(Dialect::OpenAiChat, Dialect::Gemini),
                &state,
                &resources,
                &Default::default(),
            ))
            .unwrap();
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
            if case["name"] == "truncated_tool_result" {
                assert_eq!(
                    value["contents"][0]["parts"][0]["functionResponse"]["name"],
                    "actual_lookup"
                );
            } else {
                assert_eq!(reads, 1);
                assert_eq!(
                    value["contents"][0]["parts"][0]["inlineData"]["mimeType"],
                    "image/png"
                );
            }
            value
        };
        println!(
            "GENERATION_PARITY {}",
            json!({"name":case["name"],"accepted":true,"output":output,"saved_records":store.entries.lock().unwrap().len(),"resource_reads":reads})
        );
    }
}
