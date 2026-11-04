use super::*;
use crate::wire::{
    claude::generate_content as c,
    gemini::interactions as i,
    openai::{chat as h, responses as r},
};

/// Convert a complete upstream result back to the client's native shape.
/// `id` and `created_at` are facts allocated/captured by the invoking host.
pub fn response(
    client: OperationKey,
    upstream: OperationKey,
    body: Value,
    id: &str,
    model: &str,
    created_at: i64,
) -> Result<Converted<Value>, TransformError> {
    let mut report = Report::default();
    let value = if native(upstream) {
        let body = declared::<i::Interaction>(body)?;
        let value = from_interactions(&body, client.dialect, id, model, created_at, &mut report)?;
        match client.dialect {
            Dialect::OpenAi => declared::<r::GenerateContentResponseBody>(value)?,
            Dialect::OpenAiChat => declared::<h::GenerateContentResponseBody>(value)?,
            Dialect::Claude => declared::<c::GenerateContentResponseBody>(value)?,
            _ => {
                return Err(TransformError::unsupported(
                    "interaction.response",
                    "unsupported client dialect",
                ));
            }
        }
    } else if native(client) {
        let body = match upstream.dialect {
            Dialect::OpenAi => declared::<r::GenerateContentResponseBody>(body)?,
            Dialect::OpenAiChat => declared::<h::GenerateContentResponseBody>(body)?,
            Dialect::Claude => declared::<c::GenerateContentResponseBody>(body)?,
            _ => {
                return Err(TransformError::unsupported(
                    "interaction.response",
                    "unsupported upstream dialect",
                ));
            }
        };
        declared::<i::Interaction>(to_interactions(
            &body,
            upstream.dialect,
            id,
            model,
            created_at,
            &mut report,
        )?)?
    } else {
        return Err(TransformError::unsupported(
            "interaction.response",
            "pair must have one native Interactions side",
        ));
    };
    Ok(Converted { value, report })
}

fn n(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or_default()
}
fn content_text(value: &Value) -> String {
    array(value)
        .filter(|p| p["type"] == "text")
        .map(|p| string(&p["text"]))
        .collect::<Vec<_>>()
        .join("")
}
fn response_usage(body: &Value, dialect: Dialect) -> Value {
    let usage = &body["usage"];
    if !usage.is_object() {
        return Value::Null;
    }
    let (input, cached, output, thoughts) = match dialect {
        Dialect::OpenAi => (
            n(usage, "input_tokens"),
            usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(Value::as_u64),
            n(usage, "output_tokens"),
            usage
                .pointer("/output_tokens_details/reasoning_tokens")
                .and_then(Value::as_u64),
        ),
        Dialect::OpenAiChat => (
            n(usage, "prompt_tokens"),
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_u64),
            n(usage, "completion_tokens"),
            usage
                .pointer("/completion_tokens_details/reasoning_tokens")
                .and_then(Value::as_u64),
        ),
        Dialect::Claude => (
            n(usage, "input_tokens")
                .saturating_add(n(usage, "cache_read_input_tokens"))
                .saturating_add(n(usage, "cache_creation_input_tokens")),
            usage["cache_read_input_tokens"].as_u64(),
            n(usage, "output_tokens"),
            None,
        ),
        _ => unreachable!(),
    };
    let mut out = json!({"total_input_tokens":input,"total_output_tokens":output.saturating_sub(thoughts.unwrap_or_default()),"total_tokens":input.saturating_add(output)});
    if let Some(cached) = cached {
        out["total_cached_tokens"] = json!(cached);
    }
    if let Some(thoughts) = thoughts {
        out["total_thought_tokens"] = json!(thoughts);
    }
    out
}

fn to_interactions(
    body: &Value,
    dialect: Dialect,
    id: &str,
    model: &str,
    created_at: i64,
    report: &mut Report,
) -> Result<Value, TransformError> {
    let mut steps = Vec::new();
    let incomplete;
    match dialect {
        Dialect::OpenAi => {
            if body.get("error").is_some_and(|error| !error.is_null()) {
                return Err(TransformError::upstream(
                    "response.error",
                    serde_json::from_value::<r::ResponseError>(body["error"].clone())?,
                ));
            }
            incomplete = body["status"] == "incomplete";
            for item in array(&body["output"]) {
                match string(&item["type"]) {
                    "message" => {
                        let mut content=Vec::new();
                        for part in array(&item["content"]) {
                            if part["type"]=="output_text" {content.push(json!({"type":"text","text":part["text"]}));}
                            else if part["type"]=="refusal" { content.push(json!({"type":"text","text":part["refusal"]})); report.changed("output.refusal", "refusal text retained as a native text step"); }
                            else {report.omitted("output.content", "media output has no native text-step equivalent");}
                        }
                        if !content.is_empty(){steps.push(json!({"type":"model_output","content":content}));}
                    }
                    "function_call" => steps.push(json!({"type":"function_call","id":item["call_id"],"name":item["name"],"arguments":args(&item["arguments"])?})),
                    "reasoning" => { let summary:Vec<Value>=array(&item["summary"]).map(|p|json!({"type":"text","text":p["text"]})).collect(); if !summary.is_empty(){steps.push(json!({"type":"thought","summary":summary}));} report.omitted("output.encrypted_content","foreign ciphertext is not an Interactions signature"); }
                    kind => report.omitted(format!("output.{kind}"),"hosted output has no native Interactions step equivalent"),
                }
            }
        }
        Dialect::OpenAiChat => {
            let choices: Vec<_> = array(&body["choices"]).collect();
            if choices.len() != 1 {
                return Err(TransformError::unsupported(
                    "choices",
                    "Interactions represents one result; multi-choice responses require explicit fan-out",
                ));
            }
            let choice = choices[0];
            incomplete = choice["finish_reason"] == "length";
            let message = &choice["message"];
            if let Some(text) = message["content"].as_str() {
                steps.push(json!({"type":"model_output","content":[{"type":"text","text":text}]}));
            }
            if let Some(refusal) = message["refusal"].as_str().filter(|text| !text.is_empty()) {
                steps.push(
                    json!({"type":"model_output","content":[{"type":"text","text":refusal}]}),
                );
                report.changed("refusal", "refusal text retained as a native text step");
            }
            let reasoning = message["reasoning_content"]
                .as_str()
                .or_else(|| message["reasoning"].as_str());
            if let Some(text) = reasoning {
                steps.push(json!({"type":"thought","summary":[{"type":"text","text":text}]}));
            }
            for call in array(&message["tool_calls"]) {
                if call["type"] == "function" {
                    steps.push(json!({"type":"function_call","id":call["id"],"name":call["function"]["name"],"arguments":args(&call["function"]["arguments"])?}));
                } else {
                    report.omitted(
                        "tool_calls.custom",
                        "custom calls have no native function-call equivalent",
                    );
                }
            }
        }
        Dialect::Claude => {
            incomplete = body["stop_reason"] == "max_tokens";
            for block in array(&body["content"]) {
                match string(&block["type"]) {
                    "text"=>steps.push(json!({"type":"model_output","content":[{"type":"text","text":block["text"]}]})),
                    "thinking"=>{steps.push(json!({"type":"thought","summary":[{"type":"text","text":block["thinking"]}]}));report.omitted("thinking.signature","Claude signatures are not native Interactions signatures");},
                    "tool_use"=>steps.push(json!({"type":"function_call","id":block["id"],"name":block["name"],"arguments":block["input"]})),
                    kind=>report.omitted(format!("content.{kind}"),"hosted output has no native Interactions step equivalent"),
                }
            }
        }
        _ => unreachable!(),
    }
    let tools = steps.iter().any(|s| s["type"] == "function_call");
    let status = if incomplete {
        "incomplete"
    } else if tools {
        "requires_action"
    } else {
        "completed"
    };
    let date = time::OffsetDateTime::from_unix_timestamp(created_at)
        .ok()
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        });
    let mut out = json!({"id":format!("interaction_{id}"),"object":"interaction","model":body["model"].as_str().unwrap_or(model),"status":status,"steps":steps,"usage":response_usage(body,dialect)});
    if let Some(date) = date {
        out["created"] = json!(date);
        out["updated"] = out["created"].clone();
    }
    report.changed(
        "id",
        "stateless converted interaction identity; no stored upstream resource exists",
    );
    Ok(out)
}

fn from_interactions(
    body: &Value,
    dialect: Dialect,
    id: &str,
    model: &str,
    created_at: i64,
    report: &mut Report,
) -> Result<Value, TransformError> {
    if matches!(body["status"].as_str(), Some("failed" | "cancelled")) {
        let error = array(&body["errors"]).next().cloned().unwrap_or_else(||json!({"code":body["status"],"message":format!("upstream interaction {}",string(&body["status"]))}));
        return Err(TransformError::upstream(
            "interaction.error",
            serde_json::from_value::<i::InteractionError>(error)?,
        ));
    }
    if !matches!(
        body["status"].as_str(),
        Some("completed" | "requires_action" | "incomplete" | "requires_continuation")
    ) {
        return Err(TransformError::unsupported(
            "interaction.status",
            "running, failed or cancelled resources are not complete generation responses",
        ));
    }
    let model = body["model"].as_str().unwrap_or(model);
    let usage = &body["usage"];
    let input = n(usage, "total_input_tokens");
    let cached = n(usage, "total_cached_tokens");
    let thoughts = n(usage, "total_thought_tokens");
    let output = n(usage, "total_output_tokens").saturating_add(thoughts);
    let incomplete = matches!(
        body["status"].as_str(),
        Some("incomplete" | "requires_continuation")
    );
    let tool_calls = array(&body["steps"]).any(|s| s["type"] == "function_call");
    let mut content = Vec::new();
    let mut items = Vec::new();
    let mut calls = Vec::new();
    let mut details = Vec::new();
    let mut message_text = String::new();
    for (index, step) in array(&body["steps"]).enumerate() {
        match string(&step["type"]) {
            "model_output" => {
                let text = content_text(&step["content"]);
                for part in array(&step["content"]) {
                    if part["type"] != "text" {
                        report.omitted(
                            "output.media",
                            "target generation result has no equivalent media field",
                        );
                    }
                }
                if !text.is_empty() {
                    message_text.push_str(&text);
                    content.push(json!({"type":"text","text":text}));
                    items.push(json!({"type":"message","id":format!("msg_{id}_{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[],"logprobs":[]}]}));
                }
            }
            "thought" => {
                let text = content_text(&step["summary"]);
                let signature = step["signature"].as_str().unwrap_or_default();
                content.push(json!({"type":"thinking","thinking":text,"signature":format!("{SIGNATURE}{signature}")}));
                items.push(json!({"type":"reasoning","id":format!("rs_{id}_{index}"),"summary":[{"type":"summary_text","text":text}],"encrypted_content":format!("{SIGNATURE}{signature}")}));
                details.push(json!({"type":"reasoning.text","format":REASONING_FORMAT,"index":index,"text":text,"signature":signature}));
            }
            "function_call" => {
                if let Some(signature) = step["signature"].as_str() {
                    use base64::Engine;
                    let carrier = format!(
                        "{SIGNATURE}call:{}:{signature}",
                        base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .encode(string(&step["id"]))
                    );
                    content.push(json!({"type":"thinking","thinking":"","signature":carrier}));
                    items.push(json!({"type":"reasoning","id":format!("rs_call_{id}_{index}"),"summary":[],"encrypted_content":carrier}));
                    details.push(json!({"type":"reasoning.encrypted","format":REASONING_FORMAT,"index":index,"id":step["id"],"data":signature}));
                }
                content.push(json!({"type":"tool_use","id":claude_call_id(&step["id"]),"name":step["name"],"input":step["arguments"]}));
                items.push(json!({"type":"function_call","id":format!("fc_{id}_{index}"),"call_id":step["id"],"name":step["name"],"arguments":arguments(&step["arguments"]),"status":"completed"}));
                calls.push(json!({"id":step["id"],"type":"function","function":{"name":step["name"],"arguments":arguments(&step["arguments"])}}));
            }
            kind => report.omitted(
                format!("steps.{kind}"),
                "native hosted tool activity has no target client-tool equivalent",
            ),
        }
    }
    let usage_present = usage.is_object();
    Ok(match dialect {
        Dialect::Claude => {
            if !usage_present {
                return Err(TransformError::missing_metadata("interaction.usage"));
            }
            json!({"id":format!("msg_{id}"),"type":"message","role":"assistant","model":model,"content":content,"stop_reason":if incomplete{"max_tokens"}else if tool_calls{"tool_use"}else{"end_turn"},"stop_sequence":null,"usage":{"input_tokens":input.saturating_sub(cached),"cache_read_input_tokens":cached,"output_tokens":output}})
        }
        Dialect::OpenAiChat => {
            let mut message = json!({"role":"assistant","content":if message_text.is_empty(){Value::Null}else{json!(message_text)},"refusal":null});
            if !calls.is_empty() {
                message["tool_calls"] = json!(calls);
            }
            if !details.is_empty() {
                message["reasoning_details"] = json!(details);
            }
            let mut out = json!({"id":format!("chatcmpl_{id}"),"object":"chat.completion","created":created_at,"model":model,"choices":[{"index":0,"message":message,"finish_reason":if incomplete{"length"}else if tool_calls{"tool_calls"}else{"stop"},"logprobs":null}]});
            if usage_present {
                out["usage"] = json!({"prompt_tokens":input,"completion_tokens":output,"total_tokens":input.saturating_add(output),"prompt_tokens_details":{"cached_tokens":cached},"completion_tokens_details":{"reasoning_tokens":thoughts}});
            }
            out
        }
        Dialect::OpenAi => {
            let mut out = json!({"id":format!("resp_{id}"),"object":"response","created_at":created_at,"model":model,"status":if incomplete{"incomplete"}else{"completed"},"output":items,"error":null,"incomplete_details":if incomplete{json!({"reason":"max_output_tokens"})}else{Value::Null},"instructions":null,"metadata":{},"parallel_tool_calls":true,"temperature":null,"top_p":null,"tool_choice":"auto","tools":[]});
            if usage_present {
                out["usage"] = json!({"input_tokens":input,"output_tokens":output,"total_tokens":input.saturating_add(output),"input_tokens_details":{"cached_tokens":cached,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":thoughts}});
            }
            out
        }
        _ => {
            return Err(TransformError::unsupported(
                "interaction.response",
                "unsupported target dialect",
            ));
        }
    })
}
