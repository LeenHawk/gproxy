use gproxy_client_sdk::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[test]
fn interruption_and_future_content_round_trip_in_version_one() {
    let document = json!({"schema_version":"1","messages":[{"role":"assistant","content":[
        {"type":"interrupted","played_ms":120,"provider_metadata":{}},
        {"type":"interrupted","played_ms":null,"provider_metadata":{}},
        {"type":"future_record","payload":{"keep":true},"provider_metadata":{}}
    ]}]});
    let conversation: Conversation = serde_json::from_value(document.clone()).unwrap();
    let Conversation::V1 { messages } = &conversation;
    assert!(matches!(
        messages[0].content[0].content,
        Content::Interrupted(Interruption {
            played_ms: Some(120)
        })
    ));
    assert!(matches!(
        messages[0].content[1].content,
        Content::Interrupted(Interruption { played_ms: None })
    ));
    assert!(
        matches!(&messages[0].content[2].content, Content::Unknown(value) if value["type"] == "future_record")
    );
    assert_eq!(serde_json::to_value(conversation).unwrap(), document);
}

#[test]
fn an_older_content_reader_preserves_the_new_variant_as_unknown() {
    // The pre-extension reader uses the same tagged enum and raw fallback.
    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum OlderContent {
        Text(Text),
        Opaque,
        #[serde(untagged)]
        Unknown(Value),
    }
    let value = json!({"type":"interrupted","played_ms":42});
    let old: OlderContent = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(&old, OlderContent::Unknown(raw) if raw == &value));
    let saved = serde_json::to_value(old).unwrap();
    assert_eq!(
        serde_json::from_value::<Content>(saved).unwrap(),
        Content::Interrupted(Interruption {
            played_ms: Some(42)
        })
    );
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn all_chat_renderers_silently_exclude_records_and_their_metadata() {
    let mut marker = Block::new(Content::Interrupted(Interruption {
        played_ms: Some(80),
    }));
    marker.cache = Some(CacheBreakpoint {
        ttl: Some(CacheTtl::Unknown("archive-only".into())),
    });
    // Archive-only records must bypass even native metadata decoding/diagnostics.
    for vendor in [Vendor::Claude, Vendor::Openai, Vendor::Gemini] {
        marker.provider_metadata.insert(
            vendor,
            serde_json::from_value(json!({"native":42,"chat":false})).unwrap(),
        );
    }
    for dialect in [
        Dialect::Claude,
        Dialect::Openai,
        Dialect::OpenaiChat,
        Dialect::Gemini,
    ] {
        let mut request = GenerateRequest::new(
            Model {
                id: "route".into(),
                dialect,
            },
            vec![
                Message {
                    role: Role::User,
                    content: vec![Block::text("question")],
                },
                Message {
                    role: Role::Assistant,
                    content: vec![Block::text("answer")],
                },
            ],
        );
        request.settings.max_output_tokens = Some(16);
        let baseline = dialects::render(&request, false).unwrap();
        for message in &mut request.messages {
            message.content.insert(0, marker.clone());
            message.content.push(marker.clone());
        }
        for role in [
            Role::Assistant,
            Role::User,
            Role::System,
            Role::Tool,
            Role::Unknown("archive".into()),
        ] {
            request.messages.insert(
                1,
                Message {
                    role,
                    content: vec![marker.clone()],
                },
            );
        }
        let rendered = dialects::render(&request, false).unwrap();
        assert_eq!(rendered.request, baseline.request, "{dialect:?}");
        assert_eq!(rendered.warnings, baseline.warnings, "{dialect:?}");
        assert!(rendered.warnings.is_empty(), "{dialect:?}");
    }
}
