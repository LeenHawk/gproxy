use gproxy_protocol::{
    ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily, claude,
};

#[test]
fn external_consumers_construct_extensible_wire_types_with_builders() {
    let message = claude::MessageParam::builder()
        .role(claude::MessageRole::Known(claude::MessageRoleKnown::User))
        .content(claude::StringOrArray::String("hello".into()))
        .build()
        .unwrap();
    let request = claude::CreateMessageRequestBody::builder()
        .model("claude-sonnet-4-6".to_owned().into())
        .messages(vec![message])
        .max_tokens(64)
        .build()
        .unwrap();

    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.max_tokens, 64);
}

#[test]
fn operation_key_rejects_inconsistent_kind_combinations() {
    assert!(
        OperationKey::try_new(
            Operation::Rerank,
            OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat),
        )
        .is_err()
    );
    assert!(
        OperationKey::try_new(
            Operation::GenerateContent,
            OperationKind::Family(WireFamily::OpenAi),
        )
        .is_err()
    );
}
