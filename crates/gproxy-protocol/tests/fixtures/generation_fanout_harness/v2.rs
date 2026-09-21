use serde_json::{Value, json};
fn main() {
    let cases: Value =
        serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        println!(
            "{}",
            match run(case) {
                Ok(output) => json!({"name":case["name"],"accepted":true,"output":output}),
                Err(error) => json!({"name":case["name"],"accepted":false,"error":error}),
            }
        );
    }
}
fn run(case: &Value) -> Result<Value, String> {
    use gproxy_protocol::{ContentGenerationKind as K, Operation, OperationKey};
    let source = OperationKey::content_generation(
        Operation::GenerateContent,
        if case["client"] == "chat" {
            K::OpenAiChatCompletions
        } else {
            K::GeminiGenerateContent
        },
    );
    let target = OperationKey::content_generation(
        Operation::GenerateContent,
        if case["upstream"] == "claude" {
            K::ClaudeMessages
        } else {
            K::OpenAiResponses
        },
    );
    let context = gproxy_transform::TransformContext::new(source, target);
    let pair = gproxy_transform::resolve(source, target).map_err(|e| e.to_string())?;
    let bytes = gproxy_transform::dispatch::request_bytes(
        pair,
        &context,
        &serde_json::to_vec(&case["input"]).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
