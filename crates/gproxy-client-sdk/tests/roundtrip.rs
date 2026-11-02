mod support;
use gproxy_client_sdk::*;
use serde_json::json;

#[test]
fn persistence_version_is_explicit_and_unknown_versions_fail() {
    let messages = vec![Message {
        role: Role::User,
        content: vec![Block::text("hello")],
    }];
    let json = serde_json::to_value(Conversation::new(messages.clone())).unwrap();
    assert_eq!(
        json,
        json!({"schema_version":"1","messages":[{"role":"user","content":[{"type":"text","text":"hello","provider_metadata":{}}]}]})
    );
    assert_eq!(
        serde_json::from_value::<Conversation>(json)
            .unwrap()
            .into_messages(),
        messages
    );
    assert!(
        serde_json::from_value::<Conversation>(json!({"schema_version":"2","messages":[]}))
            .is_err()
    );
}

#[cfg(feature = "claude")]
#[test]
fn claude_same_vendor_roundtrip_including_signed_and_redacted_thinking() {
    let native = support::claude();
    let parsed = dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert_eq!(parsed.usage.reasoning_tokens, Some(2));
    let request = support::request(Dialect::Claude, support::persisted(parsed));
    let rendered = dialects::render(&request, false).unwrap();
    assert_eq!(rendered["messages"][0]["content"], native["content"]);
}

#[cfg(feature = "openai")]
#[test]
fn responses_same_vendor_preserves_item_boundaries_encryption_and_lexical_arguments() {
    let native = support::openai();
    let result = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let request = support::request(Dialect::Openai, support::persisted(result));
    let rendered = dialects::render(&request, false).unwrap();
    assert_eq!(rendered["input"], native["output"]);
}

#[cfg(feature = "openai")]
#[test]
fn responses_refusal_is_portable_text_with_native_refusal_metadata() {
    let mut native = support::openai();
    native["output"][1]["content"] =
        json!([{"type":"refusal","refusal":"blocked","futureRefusal":true}]);
    let result = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert_eq!(result.finish_reason, FinishReason::ContentBlocked);
    assert_eq!(
        dialects::render(
            &support::request(Dialect::Openai, support::persisted(result)),
            false
        )
        .unwrap()["input"],
        native["output"]
    );
}

#[cfg(all(feature = "claude", feature = "gemini"))]
#[test]
fn edited_tool_arguments_replace_original_json_while_retaining_vendor_extensions() {
    for (dialect, mut result) in [
        (
            Dialect::Claude,
            dialects::claude::parse(serde_json::from_value(support::claude()).unwrap()).unwrap(),
        ),
        (
            Dialect::Gemini,
            dialects::gemini::parse(serde_json::from_value(support::gemini()).unwrap()).unwrap(),
        ),
    ] {
        for block in &mut result.content {
            if let Content::ToolCall(ToolCall { arguments, .. }) = &mut block.content {
                *arguments = "{}".into();
            }
        }
        let rendered = dialects::render(
            &support::request(dialect, vec![result.into_message()]),
            false,
        )
        .unwrap();
        match dialect {
            Dialect::Claude => {
                assert_eq!(rendered["messages"][0]["content"][3]["input"], json!({}));
                assert_eq!(
                    rendered["messages"][0]["content"][3]["extra_tool"],
                    json!({"x":1})
                );
            }
            Dialect::Gemini => {
                assert_eq!(
                    rendered["contents"][0]["parts"][2]["functionCall"]["args"],
                    json!({})
                );
                assert_eq!(
                    rendered["contents"][0]["parts"][2]["functionCall"]["futureArg"],
                    3
                );
            }
            #[cfg(feature = "openai")]
            Dialect::Openai => unreachable!(),
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => unreachable!(),
        }
    }
}

#[cfg(feature = "gemini")]
#[test]
fn gemini_in_band_errors_keep_the_http_status_and_use_the_native_status_name() {
    let mut decoder = stream::StreamDecoder::new(Dialect::Gemini);
    let error = decoder
        .push_json(
            &json!({"error":{"code":429,"status":"RESOURCE_EXHAUSTED","message":"quota"}})
                .to_string(),
        )
        .unwrap_err();
    assert!(matches!(error, Error::RateLimit { .. }));
}

#[cfg(feature = "gemini")]
#[test]
fn gemini_same_vendor_preserves_signatures_and_unknown_nested_media_fields() {
    let native = support::gemini();
    let result = dialects::gemini::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let request = support::request(Dialect::Gemini, support::persisted(result));
    let rendered = dialects::render(&request, false).unwrap();
    assert_eq!(
        rendered["contents"][0]["parts"],
        native["candidates"][0]["content"]["parts"]
    );
}

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn crossing_vendors_drops_private_metadata_and_options() {
    let results = [
        (
            Dialect::Claude,
            dialects::claude::parse(serde_json::from_value(support::claude()).unwrap()).unwrap(),
        ),
        (
            Dialect::Openai,
            dialects::openai::parse(serde_json::from_value(support::openai()).unwrap()).unwrap(),
        ),
        (
            Dialect::Gemini,
            dialects::gemini::parse(serde_json::from_value(support::gemini()).unwrap()).unwrap(),
        ),
    ];
    for (source, result) in results {
        for target in [Dialect::Claude, Dialect::Openai, Dialect::Gemini] {
            if source == target {
                continue;
            }
            let mut request = support::request(target, vec![result.clone().into_message()]);
            // Generic attachments cannot be represented by Claude's image/document API.
            if target == Dialect::Claude {
                request.messages[0]
                    .content
                    .retain(|b| !matches!(b.content, Content::File(_)));
            }
            support::extensions(&mut request.provider_options, source.vendor())
                .insert("secret_option".into(), json!("private"));
            let rendered = dialects::render(&request, false)
                .unwrap_or_else(|error| panic!("{source:?} -> {target:?}: {error}"))
                .to_string();
            for private in [
                "signature",
                "encrypted",
                "ciphertext",
                "\"future\":",
                "opaque_extra",
                "envelope_extra",
                "secret_option",
            ] {
                assert!(!rendered.contains(private), "leaked {private}: {rendered}");
            }
        }
    }
}

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn tools_sampling_structured_output_and_options_reach_each_native_dialect() {
    for dialect in [Dialect::Claude, Dialect::Openai, Dialect::Gemini] {
        let mut request = support::request(
            dialect,
            vec![
                Message {
                    role: Role::System,
                    content: vec![Block::text("be brief")],
                },
                Message {
                    role: Role::User,
                    content: vec![Block::text("hello")],
                },
            ],
        );
        request.tools = vec![Tool {
            name: "weather".into(),
            description: "weather".into(),
            input_schema: json!({"type":"object","properties":{}}),
        }];
        request.settings.temperature = Some(0.5);
        request.settings.tool_choice = Some(ToolChoice::Tool {
            name: "weather".into(),
        });
        request.settings.output_schema =
            Some(json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"]}));
        support::extensions(&mut request.provider_options, dialect.vendor())
            .insert("future_option".into(), json!({"keep":true}));
        let rendered = dialects::render(&request, false).unwrap();
        assert_eq!(rendered["future_option"], json!({"keep":true}));
        assert!(rendered["tools"].is_array());
        assert!(rendered.to_string().contains("weather"));
    }
}

#[cfg(feature = "gemini")]
#[test]
fn caller_blob_references_are_resolved_explicitly() {
    let mut request = support::request(
        Dialect::Gemini,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Image(Image {
                source: Asset::Reference {
                    reference: "sha256:blob".into(),
                },
                media_type: None,
                detail: Detail::Auto,
            }))],
        }],
    );
    assert!(dialects::render(&request, false).is_err());
    futures_executor::block_on(request.resolve_assets(|reference| async move {
        assert_eq!(reference, "sha256:blob");
        Ok(ResolvedAsset::Bytes {
            media_type: "image/png".into(),
            bytes: vec![1, 2, 3],
        })
    }))
    .unwrap();
    assert_eq!(
        dialects::render(&request, false).unwrap()["contents"][0]["parts"][0]["inlineData"]["data"],
        "AQID"
    );
}

#[cfg(feature = "claude")]
#[test]
fn claude_stream_aggregates_to_the_buffered_result_and_can_be_replayed() {
    let native = support::claude();
    let events = support::decode(Dialect::Claude, support::claude_events(&native));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ReasoningDelta { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ToolCallArgsDelta { .. }))
    );
    let result = support::aggregate(events);
    assert_eq!(
        result,
        dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap()
    );
    let rendered = dialects::render(
        &support::request(Dialect::Claude, support::persisted(result)),
        false,
    )
    .unwrap();
    assert_eq!(rendered["messages"][0]["content"], native["content"]);
}

#[cfg(feature = "openai")]
#[test]
fn responses_stream_aggregates_to_the_buffered_result_and_can_be_replayed() {
    let native = support::openai();
    let events = support::decode(Dialect::Openai, support::openai_events(&native));
    assert!(events.iter().any(|e| matches!(e, Event::TextDelta { .. })));
    let result = support::aggregate(events);
    assert_eq!(
        result,
        dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap()
    );
    assert_eq!(
        dialects::render(
            &support::request(Dialect::Openai, support::persisted(result)),
            false
        )
        .unwrap()["input"],
        native["output"]
    );
}

#[cfg(feature = "gemini")]
#[test]
fn gemini_stream_keeps_native_part_boundaries_and_late_usage() {
    let native = support::gemini();
    let mut chunks = native["candidates"][0]["content"]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| json!({"candidates":[{"content":{"role":"model","parts":[part]}}]}))
        .collect::<Vec<_>>();
    chunks.push(json!({"candidates":[{"finishReason":"STOP"}]}));
    chunks.push(json!({"usageMetadata":native["usageMetadata"]}));
    let result = support::aggregate(support::decode(Dialect::Gemini, chunks));
    assert_eq!(
        result,
        dialects::gemini::parse(serde_json::from_value(native.clone()).unwrap()).unwrap()
    );
    assert_eq!(
        dialects::render(
            &support::request(Dialect::Gemini, support::persisted(result)),
            false
        )
        .unwrap()["contents"][0]["parts"],
        native["candidates"][0]["content"]["parts"]
    );
}

#[cfg(feature = "openai")]
#[test]
fn truncated_streams_and_upstream_error_events_are_errors() {
    let decoder = stream::StreamDecoder::new(Dialect::Openai);
    assert!(decoder.finish().is_err());
    let mut decoder = stream::StreamDecoder::new(Dialect::Openai);
    let error = decoder.push_json(&json!({"type":"error","sequence_number":1,"code":"context_length_exceeded","message":"too long","param":null}).to_string()).unwrap_err();
    assert!(matches!(error, Error::ContextTooLong { .. }), "{error:?}");
}

#[cfg(feature = "ts")]
#[test]
fn typescript_exports_cover_the_persisted_and_streaming_ir() {
    use ts_rs::TS;
    assert!(Conversation::decl(&ts_rs::Config::new()).contains("\"schema_version\": \"1\""));
    let Some(out) = std::env::var_os("GPROXY_TS_OUT").map(std::path::PathBuf::from) else {
        return;
    };
    export_types(&out).unwrap();
    let declaration = std::fs::read_to_string(out.join("Conversation.ts")).unwrap();
    assert!(declaration.contains("\"schema_version\": \"1\""));
    for name in [
        "ContentBlock",
        "Content",
        "Document",
        "Audio",
        "Video",
        "ToolOutput",
        "Citation",
        "CacheBreakpoint",
        "GenerateRequest",
        "GenerateResult",
        "Event",
        "Error",
        "Asset",
    ] {
        assert!(out.join(format!("{name}.ts")).is_file());
    }
}
