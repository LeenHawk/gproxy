mod support;
use gproxy_client_sdk::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use support::content::document;

fn preserves<T: Serialize + DeserializeOwned>(value: Value) {
    let decoded: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn future_content_roles_vendor_keys_and_cache_values_survive_a_v1_ledger() {
    let value = json!({
        "schema_version":"1",
        "messages":[{
            "role":"future_role",
            "content":[{
                "type":"future_media",
                "payload":{"data":[1,2,3],"future":true},
                "provider_metadata":{"future_vendor":{"signature":"untouched"}},
                "cache":{"ttl":"one_week"}
            }]
        }]
    });
    let conversation: Conversation = serde_json::from_value(value.clone()).unwrap();
    let Conversation::V1 { messages } = &conversation;
    assert!(matches!(&messages[0].role,Role::Unknown(role) if role=="future_role"));
    assert!(matches!(
        &messages[0].content[0].content,
        Content::Unknown(_)
    ));
    assert!(
        messages[0].content[0]
            .provider_metadata
            .contains_key(&Vendor::Unknown("future_vendor".into()))
    );
    assert_eq!(serde_json::to_value(conversation).unwrap(), value);
}

#[test]
fn unknown_finish_categories_keep_both_unified_and_native_values() {
    let value = json!({"unified":"future_completion","raw":"native-future-value"});
    let reason: FinishReason = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(&reason.unified,FinishReasonKind::Unknown(kind) if kind=="future_completion"));
    assert_eq!(serde_json::to_value(reason).unwrap(), value);
    assert!(
        serde_json::from_value::<Conversation>(json!({"schema_version":"2","messages":[]}))
            .is_err()
    );
}

#[test]
fn nested_persisted_enum_extensions_keep_their_raw_values() {
    let future = json!({"type":"future_variant","payload":{"format":"new","bytes":[1,2]}});
    preserves::<Asset>(future.clone());
    preserves::<DocumentSource>(future.clone());
    preserves::<AudioFormat>(future.clone());
    preserves::<ToolOutput>(future.clone());
    preserves::<DocumentReference>(future.clone());
    preserves::<DocumentRange>(future.clone());
    preserves::<CitationSource>(future.clone());
    preserves::<ToolResultContent>(future.clone());
    preserves::<ChunkContent>(future.clone());
    preserves::<ToolChoice>(future);
    preserves::<Detail>(json!("future_detail"));
    preserves::<Modality>(json!("future_modality"));
    preserves::<ReasoningEffort>(json!("future_effort"));
}

#[cfg(feature = "openai")]
#[test]
fn future_content_is_preserved_for_storage_and_explicitly_warned_when_not_sent() {
    let raw = json!({"type":"future_content","value":{"untouched":true},"provider_metadata":{}});
    let block: Block = serde_json::from_value(raw.clone()).unwrap();
    let request = GenerateRequest::new(
        Model {
            id: "route".into(),
            dialect: Dialect::Openai,
        },
        vec![Message {
            role: Role::User,
            content: vec![block],
        }],
    );
    let rendered = dialects::openai::render(&request, false).unwrap();
    assert!(
        rendered
            .warnings
            .iter()
            .any(|w| w.field == "messages[0].content[0]" && w.kind == WarningKind::Unsupported)
    );
    assert_eq!(
        serde_json::to_value(&request.messages[0].content[0]).unwrap(),
        raw
    );
}

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
        "Interruption",
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
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn version_one_persists_the_rich_content_graph() {
    let doc = ContentBlock::new(document(DocumentSource::Text {
        text: "evidence".into(),
    }));
    let mut text = Block::text("claim");
    if let Content::Text(value) = &mut text.content {
        value.citations.push(Citation {
            source: CitationSource::Document {
                document: DocumentReference::Block {
                    document: Box::new(doc),
                },
                range: Some(DocumentRange::Chars { start: 0, end: 8 }),
            },
            title: Some("source".into()),
            cited_text: Some("evidence".into()),
            text_range: Some(TextRange { start: 0, end: 5 }),
            provider_metadata: ProviderMetadata::new(),
        });
    }
    text.cache = Some(CacheBreakpoint { ttl: None });
    let conversation = Conversation::new(vec![Message {
        role: Role::Assistant,
        content: vec![text],
    }]);
    let value = serde_json::to_value(&conversation).unwrap();
    assert_eq!(value["schema_version"], "1");
    assert_eq!(
        serde_json::from_value::<Conversation>(value).unwrap(),
        conversation
    );
}
