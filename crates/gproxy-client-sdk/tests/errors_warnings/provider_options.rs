use super::*;

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
        .unwrap()
        .request;
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
        let rendered = dialects::render(&request, false).unwrap().request;
        assert_eq!(rendered["future_option"], json!({"keep":true}));
        assert!(rendered["tools"].is_array());
        assert!(rendered.to_string().contains("weather"));
    }
}

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn metadata_keeps_native_fields_without_portable_payload_duplicates() {
    for (vendor, result) in [
        (
            Vendor::Claude,
            dialects::claude::parse(serde_json::from_value(support::claude()).unwrap()).unwrap(),
        ),
        (
            Vendor::Openai,
            dialects::openai::parse(serde_json::from_value(support::openai()).unwrap()).unwrap(),
        ),
        (
            Vendor::Gemini,
            dialects::gemini::parse(serde_json::from_value(support::gemini()).unwrap()).unwrap(),
        ),
    ] {
        for block in result.content {
            let native = &block.provider_metadata[&vendor]["native"];
            match block.content {
                Content::Text(Text { .. }) => {
                    assert!(native.get("text").is_none());
                    assert!(native.get("refusal").is_none());
                }
                Content::ToolCall(ToolCall { .. }) if vendor != Vendor::Gemini => {
                    assert!(native.get("arguments").is_none());
                    assert!(native.get("input").is_none());
                }
                Content::Image(Image {
                    source: Asset::Inline { .. },
                    ..
                }) if vendor == Vendor::Gemini => {
                    assert!(native["inlineData"].get("data").is_none())
                }
                _ => {}
            }
        }
    }
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn vendor_options_are_typed_and_keep_native_nulls_and_extensions() {
    let options: ProviderOptions = serde_json::from_value(json!({
        "claude": {
            "max_tokens": 64,
            "thinking": {
                "type": "adaptive"
            },
            "tools": [
                {
                    "name": "native",
                    "input_schema": {
                        "type": "object"
                    }
                }
            ],
            "claude_only": true
        },
        "openai": {
            "responses": {
                "reasoning": {
                    "effort": "high"
                },
                "include": [
                    "reasoning.encrypted_content"
                ],
                "store": null,
                "tools": [
                    {
                        "type": "function",
                        "name": "native",
                        "parameters": {
                            "type": "object"
                        },
                        "strict": null
                    }
                ],
                "responses_only": 2
            },
            "chat": {
                "audio": {
                    "voice": "alloy",
                    "format": "wav",
                    "audio_extra": 7
                },
                "tools": [
                    {
                        "type": "function",
                        "function": {
                            "name": "native",
                            "parameters": {
                                "type": "object"
                            }
                        }
                    }
                ],
                "seed": 3,
                "chat_only": 4
            },
            "openai_only": 5
        },
        "gemini": {
            "generationConfig": {
                "temperature": 0.2,
                "speechConfig": {
                    "languageCode": "en-US",
                    "speech_extra": 8,
                    "voiceConfig": {
                        "voice_extra": 9,
                        "prebuiltVoiceConfig": {
                            "voiceName": "Puck",
                            "prebuilt_extra": 10
                        }
                    }
                }
            },
            "tools": [
                {
                    "googleSearch": {}
                }
            ],
            "cachedContent": "cachedContents/one",
            "gemini_only": 6
        }
    }))
    .unwrap();
    assert_eq!(options.openai.responses.store, Some(None));
    let persisted: ProviderOptions =
        serde_json::from_value(serde_json::to_value(&options).unwrap()).unwrap();
    assert_eq!(options, persisted);
    for dialect in [
        Dialect::Claude,
        Dialect::Openai,
        Dialect::OpenaiChat,
        Dialect::Gemini,
    ] {
        let mut request = GenerateRequest::new(
            Model {
                id: "model".into(),
                dialect,
            },
            vec![Message {
                role: Role::User,
                content: vec![Block::text("hello")],
            }],
        );
        request.provider_options = options.clone();
        request.tools = vec![Tool {
            name: "portable".into(),
            description: String::new(),
            input_schema: json!({"type":"object"}),
        }];
        let native = dialects::render(&request, true).unwrap().request;
        assert_eq!(native["tools"].as_array().unwrap().len(), 2);
        match dialect {
            Dialect::Claude => {
                assert_eq!(native["thinking"], json!({"type":"adaptive"}));
                assert_eq!(native["max_tokens"], 64);
                assert!(native.get("openai_only").is_none());
            }
            Dialect::Openai => {
                assert_eq!(native["store"], Value::Null);
                assert_eq!(native["reasoning"]["effort"], "high");
                assert_eq!(native["responses_only"], 2);
                assert_eq!(native["openai_only"], 5);
                assert!(native.get("chat_only").is_none());
            }
            Dialect::OpenaiChat => {
                assert_eq!(native["audio"]["format"], "wav");
                assert_eq!(native["seed"], 3);
                assert_eq!(native["stream_options"]["include_usage"], true);
                assert!(native.get("responses_only").is_none());
            }
            Dialect::Gemini => {
                assert_eq!(native["cachedContent"], "cachedContents/one");
                assert_eq!(native["generationConfig"]["temperature"], 0.2);
                assert!(native.get("openai_only").is_none());
            }
        }
        if matches!(dialect, Dialect::OpenaiChat | Dialect::Gemini) {
            request.settings.speech = Some(SpeechSettings {
                voice: if dialect == Dialect::Gemini {
                    "Puck"
                } else {
                    "nova"
                }
                .into(),
                format: AudioFormat::pcm(24000, 1),
            });
            let updated = dialects::render(&request, true).unwrap().request;
            if dialect == Dialect::OpenaiChat {
                assert_eq!(updated["audio"]["audio_extra"], 7);
                assert_eq!(updated["audio"]["voice"], "alloy");
            } else {
                let speech = &updated["generationConfig"]["speechConfig"];
                assert_eq!(speech["languageCode"], "en-US");
                assert_eq!(speech["speech_extra"], 8);
                assert_eq!(speech["voiceConfig"]["voice_extra"], 9);
                assert_eq!(
                    speech["voiceConfig"]["prebuiltVoiceConfig"]["prebuilt_extra"],
                    10
                );
                assert_eq!(
                    speech["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"],
                    "Puck"
                );
            }
        }
    }
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn native_options_override_portable_controls_with_diagnostics() {
    let mut request = support::request(Dialect::Gemini, vec![]);
    request.settings.temperature = Some(0.8);
    request.settings.max_output_tokens = Some(90);
    request.settings.output_modalities = vec![Modality::Audio];
    request.provider_options.gemini.generation_config = Some(
        serde_json::from_value(
            json!({"temperature":0.2,"maxOutputTokens":80,"responseModalities":["TEXT"]}),
        )
        .unwrap(),
    );
    let compiled = dialects::gemini::render(&request).unwrap();
    assert_eq!(
        compiled
            .request
            .generation_config
            .as_ref()
            .unwrap()
            .temperature,
        Some(0.2)
    );
    assert_eq!(
        compiled
            .request
            .generation_config
            .as_ref()
            .unwrap()
            .max_output_tokens,
        Some(80)
    );
    for field in [
        "settings.temperature",
        "settings.max_output_tokens",
        "settings.output_modalities",
    ] {
        assert!(compiled.warnings.iter().any(|w| w.field == field));
    }
}
