mod pressure;
mod quota;
mod setup;
mod tokenizer;

fn generation_operation() -> gproxy_protocol::OperationKey {
    gproxy_protocol::OperationKey::content(
        gproxy_protocol::Operation::GenerateContent,
        gproxy_protocol::ContentGenerationKind::OpenAiChat,
    )
}
