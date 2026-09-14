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
    use gproxy_transform::typed::generate_content::{
        openai_chat_to_claude_messages as c, openai_chat_to_gemini_generate_content as g,
    };
    if let Some(prime) = case.get("prime") {
        let _ = g::response(serde_json::from_value(prime.clone()).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    if case["upstream"] == "claude" {
        return serde_json::to_value(
            c::response(serde_json::from_value(case["input"].clone()).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string());
    }
    if case["operation"] == "request" {
        serde_json::to_value(
            g::request(
                serde_json::from_value(case["input"].clone()).map_err(|e| e.to_string())?,
                gproxy_transform::typed::RequestContext::new("selected", false),
            )
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    } else {
        serde_json::to_value(
            g::response(serde_json::from_value(case["input"].clone()).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}
