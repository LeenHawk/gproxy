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
fn run(case: &Value) -> Result<Value, String> {
    let input = case["input"].clone();
    use gproxy_transform::typed::videos::{gemini_to_openai as gn, openai_to_gemini as ng};
    match case["direction"].as_str().unwrap() {
        "native_request" => serde_json::to_value(ng::create_request(
            serde_json::from_value(input).map_err(|e| e.to_string())?,
        )),
        "veo_request" => serde_json::to_value(gn::create_request(
            serde_json::from_value(input).map_err(|e| e.to_string())?,
            gproxy_transform::typed::RequestContext::new("sora-2", false),
        )),
        "veo_response" => serde_json::to_value(
            ng::response(serde_json::from_value(input).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        ),
        "native_response" => serde_json::to_value(gn::response(
            serde_json::from_value(input).map_err(|e| e.to_string())?,
        )),
        _ => unreachable!(),
    }
    .map_err(|e| e.to_string())
}
