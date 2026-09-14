use serde_json::{Value, json};
fn main() {
    let cases: Value =
        serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let result = run(case);
        println!(
            "{}",
            match result {
                Ok(output) => json!({"name":case["name"],"accepted":true,"output":output}),
                Err(error) => json!({"name":case["name"],"accepted":false,"error":error}),
            }
        );
    }
}
fn run(case: &Value) -> Result<Value, String> {
    use gproxy_protocol::{ContentGenerationKind as K, Operation, OperationKey};
    let source =
        OperationKey::content_generation(Operation::GenerateContent, K::OpenAiChatCompletions);
    let target = OperationKey::content_generation(
        Operation::GenerateContent,
        if case["upstream"] == "claude" {
            K::ClaudeMessages
        } else {
            K::GeminiGenerateContent
        },
    );
    let response_context = gproxy_transform::TransformContext::new(target, source);
    let response_pair = gproxy_transform::resolve(target, source).map_err(|e| e.to_string())?;
    let context = gproxy_transform::TransformContext::new(source, target);
    let pair = gproxy_transform::resolve(source, target).map_err(|e| e.to_string())?;
    if let Some(prime) = case.get("prime") {
        let bytes = serde_json::to_vec(prime).unwrap();
        let _ =
            gproxy_transform::dispatch::response_bytes(response_pair, &response_context, &bytes)
                .map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec(&case["input"]).unwrap();
    let output = if case["operation"] == "request" {
        gproxy_transform::dispatch::request_bytes(pair, &context, &bytes)
    } else {
        gproxy_transform::dispatch::response_bytes(response_pair, &response_context, &bytes)
    }
    .map_err(|e| e.to_string())?;
    serde_json::from_slice(&output).map_err(|e| e.to_string())
}
