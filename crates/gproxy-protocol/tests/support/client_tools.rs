use serde_json::{Value, json};

pub fn request(stream: bool) -> Value {
    json!({"model":"client-alias","stream":stream,"input":"Make the change.","parallel_tool_calls":true,"tool_choice":"auto","tools":[
        {"type":"shell","environment":{"type":"local"}},
        {"type":"apply_patch"},
        {"type":"namespace","name":"repo","description":"Repository tools","tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{"q":{"type":"string"}}},"strict":false}]},
        {"type":"tool_search","execution":"client","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}},
        {"type":"function","name":"later","parameters":{"type":"object","properties":{}},"strict":false,"defer_loading":true}
    ]})
}
pub fn response(names: &[String]) -> Value {
    let args = [
        json!({"commands":["printf hello"],"timeout_ms":1000,"max_output_length":4096}),
        json!({"type":"update_file","path":"src/a.rs","diff":"@@\n-old\n+new"}),
        json!({"q":"readme"}),
        json!({"query":"later"}),
    ];
    let calls: Vec<_> = names.iter().zip(args).enumerate().map(|(index,(name,arguments))| json!({"id":format!("native.{index}"),"type":"function","function":{"name":name,"arguments":arguments.to_string()}})).collect();
    json!({"id":"chat.native","created":7,"model":"selected","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":null,"refusal":null,"tool_calls":calls},"finish_reason":"tool_calls","logprobs":null}],"usage":{"prompt_tokens":4,"completion_tokens":8,"total_tokens":12,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}}})
}
pub fn followup(output: &Value) -> Value {
    let mut request = request(false);
    let mut items = output["output"].as_array().unwrap().clone();
    items.extend([
        json!({"type":"shell_call_output","call_id":items[0]["call_id"],"output":[{"stdout":"hello","stderr":"","outcome":{"type":"exit","exit_code":0}}]}),
        json!({"type":"apply_patch_call_output","call_id":items[1]["call_id"],"status":"failed","output":"context mismatch"}),
        json!({"type":"function_call_output","call_id":items[2]["call_id"],"name":"lookup","namespace":"repo","output":"README.md"}),
        json!({"type":"tool_search_output","call_id":items[3]["call_id"],"execution":"client","tools":[{"type":"function","name":"later","parameters":{"type":"object","properties":{}},"strict":false}]}),
    ]);
    request["input"] = Value::Array(items);
    request
}
