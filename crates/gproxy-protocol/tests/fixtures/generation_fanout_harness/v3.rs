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
    use gproxy_transform::typed::{RequestContext, generate_content as t};
    macro_rules! call {
        ($edge:ident) => {
            serde_json::to_value(
                t::$edge::request(
                    serde_json::from_value(case["input"].clone()).map_err(|e| e.to_string())?,
                    RequestContext::new("selected", false),
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        };
    }
    match (
        case["client"].as_str().unwrap(),
        case["upstream"].as_str().unwrap(),
    ) {
        ("chat", "claude") => call!(openai_chat_to_claude_messages),
        ("chat", "responses") => call!(openai_chat_to_openai_responses),
        ("gemini", "claude") => call!(gemini_generate_content_to_claude_messages),
        ("gemini", "responses") => call!(gemini_generate_content_to_openai_responses),
        _ => unreachable!(),
    }
}
