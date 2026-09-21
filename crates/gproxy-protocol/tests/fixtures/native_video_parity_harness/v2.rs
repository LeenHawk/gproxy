use serde_json::{Value, json};
fn main() {
    let file = std::env::args().nth(1).unwrap();
    let cases: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
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
use gproxy_protocol::{Operation, OperationKey, Provider};
fn run(case: &Value) -> Result<Value, String> {
    let direction = case["direction"].as_str().unwrap();
    let native = direction.starts_with("native");
    let source = OperationKey::provider(
        Operation::CreateVideo,
        if native {
            Provider::OpenAi
        } else {
            Provider::Gemini
        },
    );
    let target = OperationKey::provider(
        Operation::CreateVideo,
        if native {
            Provider::Gemini
        } else {
            Provider::OpenAi
        },
    );
    let ctx = gproxy_transform::TransformContext::new(source, target);
    let pair = gproxy_transform::resolve(source, target).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(&case["input"]).unwrap();
    let out = if direction.ends_with("request") {
        gproxy_transform::dispatch::request_bytes(pair, &ctx, &bytes)
    } else {
        gproxy_transform::dispatch::response_bytes(pair, &ctx, &bytes)
    }
    .map_err(|e| e.to_string())?;
    serde_json::from_slice(&out).map_err(|e| e.to_string())
}
