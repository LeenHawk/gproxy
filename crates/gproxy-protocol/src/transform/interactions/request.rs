use super::*;
use crate::wire::{
    claude::generate_content as c,
    gemini::interactions as i,
    openai::{chat as h, responses as r},
};

/// A directed pair, with explicit target model and Claude's required budget.
pub fn request(
    source: OperationKey,
    target: OperationKey,
    body: Value,
    model: &str,
    max_tokens: u64,
) -> Result<Converted<Value>, TransformError> {
    let mut report = Report::default();
    let value = if native(target) {
        let body = match source.dialect {
            Dialect::OpenAi => declared::<r::GenerateContentRequestBody>(body)?,
            Dialect::OpenAiChat => declared::<h::GenerateContentRequestBody>(body)?,
            Dialect::Claude => declared::<c::GenerateContentRequestBody>(body)?,
            _ => {
                return Err(TransformError::unsupported(
                    "interaction.request",
                    "unsupported source dialect",
                ));
            }
        };
        declared::<i::CreateInteractionRequestBody>(to_interactions(
            &body,
            source.dialect,
            model,
            &mut report,
        )?)?
    } else if native(source) {
        let body = declared::<i::CreateInteractionRequestBody>(body)?;
        let value = from_interactions(&body, target.dialect, model, max_tokens, &mut report)?;
        match target.dialect {
            Dialect::OpenAi => declared::<r::GenerateContentRequestBody>(value)?,
            Dialect::OpenAiChat => declared::<h::GenerateContentRequestBody>(value)?,
            Dialect::Claude => declared::<c::GenerateContentRequestBody>(value)?,
            _ => {
                return Err(TransformError::unsupported(
                    "interaction.request",
                    "unsupported target dialect",
                ));
            }
        }
    } else {
        return Err(TransformError::unsupported(
            "interaction.request",
            "pair must have one native Interactions side",
        ));
    };
    Ok(Converted { value, report })
}

fn thought(content: &Value, signature: &Value, steps: &mut Vec<Value>, report: &mut Report) {
    if let Some(signature) = signature
        .as_str()
        .and_then(|s| s.strip_prefix(SIGNATURE))
        .filter(|s| !s.is_empty())
    {
        steps.push(json!({"type":"thought","summary":[{"type":"text","text":text(content)}],"signature":signature}));
    } else {
        report.omitted("reasoning", "foreign or unsigned reasoning history cannot be replayed as a native Interactions signature");
    }
}

fn response_items(
    items: &Value,
    steps: &mut Vec<Value>,
    system: &mut Vec<String>,
    report: &mut Report,
) -> Result<(), TransformError> {
    if let Some(input) = items.as_str() {
        steps.push(json!({"type":"user_input","content":[{"type":"text","text":input}]}));
        return Ok(());
    }
    for item in array(items) {
        match string(&item["type"]) {
            "function_call" => steps.push(json!({"type":"function_call","id":item["call_id"],"name":item["name"],"arguments":args(&item["arguments"])?})),
            "function_call_output" => steps.push(json!({"type":"function_result","call_id":item["call_id"],"result":native_result(&item["output"], Dialect::OpenAi, report)?})),
            "reasoning" => thought(&item["summary"], &item["encrypted_content"], steps, report),
            "message" | "" => message(item, Dialect::OpenAi, steps, system, report)?,
            kind => report.omitted(format!("input.{kind}"), "hosted operation has no equivalent native input step"),
        }
    }
    Ok(())
}

fn message(
    item: &Value,
    dialect: Dialect,
    steps: &mut Vec<Value>,
    system: &mut Vec<String>,
    report: &mut Report,
) -> Result<(), TransformError> {
    let role = string(&item["role"]);
    if matches!(role, "system" | "developer") && steps.is_empty() {
        system.push(text(&item["content"]));
        return Ok(());
    }
    if matches!(role, "system" | "developer") {
        report.changed(
            "messages.role",
            "a mid-conversation instruction stays in place as user input",
        );
    }
    if dialect == Dialect::OpenAiChat && role == "tool" {
        steps.push(json!({"type":"function_result","call_id":item["tool_call_id"],"result":native_result(&item["content"], Dialect::OpenAiChat, report)?}));
        return Ok(());
    }
    let mut content = Vec::new();
    if dialect == Dialect::Claude && item["content"].is_array() {
        for block in array(&item["content"]) {
            match string(&block["type"]) {
                "tool_use" => {
                    if !content.is_empty() {
                        steps.push(
                            json!({"type":"model_output","content":std::mem::take(&mut content)}),
                        );
                    }
                    steps.push(json!({"type":"function_call","id":native_call_id(&block["id"]),"name":block["name"],"arguments":block["input"]}));
                }
                "tool_result" => {
                    if !content.is_empty() {
                        steps.push(
                            json!({"type":"user_input","content":std::mem::take(&mut content)}),
                        );
                    }
                    let mut result = json!({"type":"function_result","call_id":native_call_id(&block["tool_use_id"]),"result":native_result(&block["content"], Dialect::Claude, report)?});
                    copy(block, &mut result, "is_error", "is_error");
                    steps.push(result);
                }
                "thinking" => thought(&block["thinking"], &block["signature"], steps, report),
                _ => content.extend(content::to_native(&json!([block]), dialect, report)?),
            }
        }
    } else if !item["content"].is_null() {
        content = content::to_native(&item["content"], dialect, report)?;
    }
    if !content.is_empty() {
        steps.push(json!({"type":if role == "assistant" {"model_output"} else {"user_input"},"content":content}));
    }
    if dialect == Dialect::OpenAiChat {
        for detail in array(&item["reasoning_details"]) {
            if detail["format"] == REASONING_FORMAT && detail["type"] == "reasoning.encrypted" {
                use base64::Engine;
                let id =
                    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(string(&detail["id"]));
                thought(
                    &Value::Null,
                    &json!(format!("{SIGNATURE}call:{id}:{}", string(&detail["data"]))),
                    steps,
                    report,
                );
            } else if detail["format"] == REASONING_FORMAT {
                let signature = detail.get("signature").unwrap_or(&Value::Null);
                thought(
                    &detail["text"],
                    &Value::String(format!("{SIGNATURE}{}", string(signature))),
                    steps,
                    report,
                );
            }
        }
        for call in array(&item["tool_calls"]) {
            if call["type"] != "function" {
                return Err(TransformError::unsupported(
                    "tool_calls",
                    "custom tool calls require a function schema for Interactions",
                ));
            }
            steps.push(json!({"type":"function_call","id":call["id"],"name":call["function"]["name"],"arguments":args(&call["function"]["arguments"])?}));
        }
        if item.get("function_call").is_some_and(|v| !v.is_null()) {
            return Err(TransformError::unsupported(
                "function_call",
                "legacy calls have no call id; use tool_calls",
            ));
        }
    }
    Ok(())
}

fn to_interactions(
    body: &Value,
    dialect: Dialect,
    model: &str,
    report: &mut Report,
) -> Result<Value, TransformError> {
    for field in ["previous_response_id", "conversation"] {
        if body.get(field).is_some_and(|v| !v.is_null()) {
            return Err(TransformError::unsupported(
                field,
                "send explicit full history when converting to Interactions",
            ));
        }
    }
    if body["background"] == true {
        return Err(TransformError::unsupported(
            "background",
            "foreign background lifecycle cannot be mapped to an Interactions resource",
        ));
    }
    let mut steps = Vec::new();
    let mut system = Vec::new();
    match dialect {
        Dialect::OpenAi => {
            if !body["instructions"].is_null() {
                system.push(text(&body["instructions"]));
            }
            response_items(&body["input"], &mut steps, &mut system, report)?;
        }
        Dialect::Claude => {
            if !body["system"].is_null() {
                system.push(text(&body["system"]));
            }
            for item in array(&body["messages"]) {
                message(item, dialect, &mut steps, &mut system, report)?;
            }
        }
        Dialect::OpenAiChat => {
            for item in array(&body["messages"]) {
                message(item, dialect, &mut steps, &mut system, report)?;
            }
        }
        _ => unreachable!(),
    }
    restore_call_signatures(&mut steps, report)?;
    let mut out = json!({"model":model,"input":steps,"store":false});
    if !system.is_empty() {
        out["system_instruction"] = json!(system.join("\n\n"));
    }
    let mut config = json!({});
    let limit = match dialect {
        Dialect::Claude => "max_tokens",
        Dialect::OpenAiChat if !body["max_completion_tokens"].is_null() => "max_completion_tokens",
        Dialect::OpenAiChat => "max_tokens",
        _ => "max_output_tokens",
    };
    copy(body, &mut config, limit, "max_output_tokens");
    copy(body, &mut config, "seed", "seed");
    copy(
        body,
        &mut config,
        if dialect == Dialect::Claude {
            "stop_sequences"
        } else {
            "stop"
        },
        "stop_sequences",
    );
    if let Some(stop) = config["stop_sequences"].as_str() {
        config["stop_sequences"] = json!([stop]);
    }
    let effort = if dialect == Dialect::Claude {
        body.pointer("/output_config/effort")
            .and_then(Value::as_str)
    } else if dialect == Dialect::OpenAi {
        body.pointer("/reasoning/effort").and_then(Value::as_str)
    } else {
        body["reasoning_effort"].as_str()
    };
    if let Some(effort) = effort {
        config["thinking_level"] = json!(match effort {
            "none" => "minimal",
            "minimal" | "low" | "medium" | "high" => effort,
            _ => {
                report.changed(
                    "reasoning.effort",
                    "effort mapped to the highest Interactions thinking level",
                );
                "high"
            }
        });
    }
    if dialect == Dialect::Claude && body["thinking"]["type"] == "disabled" {
        config["thinking_level"] = json!("minimal");
    }
    if dialect == Dialect::Claude && body["thinking"]["type"] == "enabled" {
        config["thinking_level"] = json!("high");
        report.changed(
            "thinking.budget_tokens",
            "Interactions uses thinking levels instead of a token budget",
        );
    }
    if !config.as_object().unwrap().is_empty() {
        out["generation_config"] = config;
    }
    let mut tools = Vec::new();
    for tool in array(&body["tools"]) {
        let function = if dialect == Dialect::OpenAiChat {
            &tool["function"]
        } else {
            tool
        };
        if dialect == Dialect::Claude && !tool["type"].is_null() && tool["type"] != "custom"
            || dialect != Dialect::Claude && tool["type"] != "function"
        {
            report.omitted(
                "tools",
                "hosted or custom tools cannot be mapped to a native Interactions function",
            );
            continue;
        }
        let mut native = json!({"type":"function","name":function["name"]});
        copy(function, &mut native, "description", "description");
        copy(
            function,
            &mut native,
            if dialect == Dialect::Claude {
                "input_schema"
            } else {
                "parameters"
            },
            "parameters",
        );
        tools.push(native);
    }
    if !tools.is_empty() {
        out["tools"] = json!(tools);
    }
    if let Some(choice) = body.get("tool_choice").filter(|v| !v.is_null()) {
        let mode = choice
            .as_str()
            .or_else(|| choice["type"].as_str())
            .unwrap_or("auto");
        let name = if dialect == Dialect::OpenAiChat {
            choice["function"]["name"].as_str()
        } else {
            choice["name"].as_str()
        };
        out["generation_config"]["tool_choice"] = if let Some(name) = name {
            json!({"allowed_tools":{"mode":"any","tools":[name]}})
        } else {
            json!(match mode {
                "required" | "any" => "any",
                "none" => "none",
                _ => "auto",
            })
        };
    }
    let format = match dialect {
        Dialect::OpenAi => body.pointer("/text/format"),
        Dialect::OpenAiChat => body.get("response_format"),
        Dialect::Claude => body
            .pointer("/output_config/format")
            .or_else(|| body.get("output_format")),
        _ => None,
    };
    if let Some(format) = format {
        let schema = format
            .get("schema")
            .or_else(|| format.pointer("/json_schema/schema"));
        if let Some(schema) = schema {
            out["response_format"] =
                json!({"type":"text","mime_type":"application/json","schema":schema});
        } else if format["type"] == "json_object" {
            out["response_format"] = json!({"type":"text","mime_type":"application/json"});
        }
    }
    omitted(
        body,
        &[
            "temperature",
            "top_p",
            "top_k",
            "frequency_penalty",
            "presence_penalty",
            "logprobs",
            "top_logprobs",
            "parallel_tool_calls",
            "cache_control",
            "prompt_cache_options",
            "context_management",
            "service_tier",
            "n",
            "store",
        ],
        report,
    );
    Ok(out)
}

fn from_interactions(
    body: &Value,
    dialect: Dialect,
    model: &str,
    max_tokens: u64,
    report: &mut Report,
) -> Result<Value, TransformError> {
    for field in [
        "agent",
        "previous_interaction_id",
        "continuation_token",
        "environment",
        "agent_config",
    ] {
        if body.get(field).is_some_and(|value| !value.is_null()) {
            return Err(TransformError::unsupported(
                field,
                "this stateful Interactions capability requires a native upstream",
            ));
        }
    }
    if body["background"] == true || body["store"] == true {
        return Err(TransformError::unsupported(
            "background/store",
            "stored and background Interactions require a native upstream; use store:false for stateless conversion",
        ));
    }
    if body["store"].is_null() {
        report.changed(
            "store",
            "converted interactions are stateless and cannot be retrieved by id",
        );
    }
    let mut out = json!({"model":model});
    let mut messages = Vec::new();
    let mut inputs = Vec::new();
    let system = body["system_instruction"].as_str();
    if dialect == Dialect::OpenAiChat
        && let Some(system) = system
    {
        messages.push(json!({"role":"system","content":system}));
    }
    if dialect == Dialect::Claude
        && let Some(system) = system
    {
        out["system"] = json!(system);
    }
    if dialect == Dialect::OpenAi
        && let Some(system) = system
    {
        out["instructions"] = json!(system);
    }
    let input = &body["input"];
    let input_items = if input.is_string() {
        vec![json!({"type":"user_input","content":[{"type":"text","text":input}]})]
    } else if input.is_object() {
        vec![json!({"type":"user_input","content":[input]})]
    } else if array(input).all(|v| {
        matches!(
            v["type"].as_str(),
            Some("text" | "image" | "audio" | "video" | "document")
        )
    }) {
        vec![json!({"type":"user_input","content":input})]
    } else {
        array(input).cloned().collect()
    };
    for step in &input_items {
        let kind = string(&step["type"]);
        match kind {
            "user_input" | "model_output" => {
                let role = if kind == "model_output" {
                    "assistant"
                } else {
                    "user"
                };
                let mut content = content::from_native(&step["content"], dialect, report)?;
                if dialect == Dialect::OpenAi && role == "assistant" {
                    for part in &mut content {
                        if part["type"] == "input_text" {
                            part["type"] = json!("output_text");
                            part["annotations"] = json!([]);
                        }
                    }
                }
                let message = json!({"role":role,"content":content});
                if dialect == Dialect::OpenAi {
                    inputs.push(message);
                } else {
                    messages.push(message);
                }
            }
            "function_call" => {
                match dialect {
                    Dialect::OpenAi => inputs.push(json!({"type":"function_call","call_id":step["id"],"name":step["name"],"arguments":arguments(&step["arguments"])})),
                    Dialect::OpenAiChat => messages.push(json!({"role":"assistant","tool_calls":[{"type":"function","id":step["id"],"function":{"name":step["name"],"arguments":arguments(&step["arguments"])}}]})),
                    Dialect::Claude => messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":claude_call_id(&step["id"]),"name":step["name"],"input":step["arguments"]}]})),
                    _ => unreachable!(),
                }
                if let Some(signature) = step["signature"].as_str() {
                    report.omitted(
                        "function_call.signature",
                        format!(
                            "native signature {count} bytes has no foreign tool-call field",
                            count = signature.len()
                        ),
                    );
                }
            }
            "function_result" => {
                let result = foreign_result(&step["result"], dialect, report)?;
                match dialect {
                    Dialect::OpenAi => inputs.push(json!({"type":"function_call_output","call_id":step["call_id"],"output":result})),
                    Dialect::OpenAiChat => messages.push(json!({"role":"tool","tool_call_id":step["call_id"],"content":result})),
                    Dialect::Claude => { let mut block = json!({"type":"tool_result","tool_use_id":claude_call_id(&step["call_id"]),"content":result}); copy(step,&mut block,"is_error","is_error"); messages.push(json!({"role":"user","content":[block]})); }
                    _ => unreachable!(),
                }
            }
            "thought" => report.omitted(
                "input.thought",
                "native Interactions thinking is not foreign reasoning history",
            ),
            _ => {
                return Err(TransformError::unsupported(
                    format!("input.{kind}"),
                    "hosted step history requires the native Interactions upstream",
                ));
            }
        }
    }
    if dialect == Dialect::OpenAi {
        out["input"] = json!(inputs);
    } else {
        out["messages"] = json!(if dialect == Dialect::OpenAiChat {
            merge_chat_assistants(messages)
        } else {
            messages
        });
    }
    let config = &body["generation_config"];
    let budget = config["max_output_tokens"].as_u64().unwrap_or(max_tokens);
    if dialect == Dialect::Claude {
        out["max_tokens"] = json!(budget);
    } else {
        copy(
            config,
            &mut out,
            "max_output_tokens",
            if dialect == Dialect::OpenAi {
                "max_output_tokens"
            } else {
                "max_completion_tokens"
            },
        );
    }
    copy(
        config,
        &mut out,
        "stop_sequences",
        if dialect == Dialect::Claude {
            "stop_sequences"
        } else {
            "stop"
        },
    );
    if dialect != Dialect::Claude {
        copy(config, &mut out, "seed", "seed");
    }
    if let Some(level) = config["thinking_level"].as_str() {
        match dialect {
            Dialect::OpenAi => out["reasoning"] = json!({"effort":level}),
            Dialect::OpenAiChat => out["reasoning_effort"] = json!(level),
            Dialect::Claude if level == "minimal" => out["thinking"] = json!({"type":"disabled"}),
            Dialect::Claude => {
                out["thinking"] = json!({"type":"adaptive"});
                out["output_config"]["effort"] = json!(level);
            }
            _ => {}
        }
    }
    let mut tools = Vec::new();
    for tool in array(&body["tools"]) {
        if tool["type"] != "function" {
            report.omitted(
                "tools",
                "native hosted tool has no target function equivalent",
            );
            continue;
        }
        let mut function = json!({"name":tool["name"]});
        copy(tool, &mut function, "description", "description");
        function[if dialect == Dialect::Claude {
            "input_schema"
        } else {
            "parameters"
        }] = tool
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({"type":"object","properties":{}}));
        if dialect == Dialect::OpenAiChat {
            tools.push(json!({"type":"function","function":function}));
        } else {
            if dialect == Dialect::OpenAi {
                function["type"] = json!("function");
            }
            tools.push(function);
        }
    }
    if !tools.is_empty() {
        out["tools"] = json!(tools);
    }
    if let Some(choice) = config.get("tool_choice") {
        let mode = choice
            .as_str()
            .or_else(|| {
                choice
                    .pointer("/allowed_tools/mode")
                    .and_then(Value::as_str)
            })
            .unwrap_or("auto");
        let names = choice
            .pointer("/allowed_tools/tools")
            .and_then(Value::as_array);
        out["tool_choice"] = if let Some(names) = names
            && names.len() == 1
            && mode == "any"
        {
            match dialect {
                Dialect::OpenAi => json!({"type":"function","name":names[0]}),
                Dialect::OpenAiChat => json!({"type":"function","function":{"name":names[0]}}),
                _ => json!({"type":"tool","name":names[0]}),
            }
        } else if dialect == Dialect::Claude {
            json!({"type":match mode {"any"=>"any","none"=>"none",_=>"auto"}})
        } else {
            json!(match mode {
                "any" => "required",
                "none" => "none",
                _ => "auto",
            })
        };
        if names.is_some_and(|names| names.len() > 1) {
            report.omitted(
                "tool_choice.allowed_tools",
                "target cannot restrict auto choice to this tool subset",
            );
        }
    }
    if let Some(format) = body.get("response_format") {
        if format["type"] == "text" && format["mime_type"] == "application/json" {
            let schema = format
                .get("schema")
                .cloned()
                .unwrap_or_else(|| json!({"type":"object"}));
            match dialect {
                Dialect::OpenAi => {
                    out["text"] =
                        json!({"format":{"type":"json_schema","name":"response","schema":schema}})
                }
                Dialect::OpenAiChat => {
                    out["response_format"] = json!({"type":"json_schema","json_schema":{"name":"response","schema":schema}})
                }
                _ => out["output_config"]["format"] = json!({"type":"json_schema","schema":schema}),
            }
        } else {
            report.omitted(
                "response_format",
                "requested output modality has no target representation",
            );
        }
    }
    omitted(
        body,
        &[
            "labels",
            "safety_settings",
            "webhook_config",
            "service_tier",
        ],
        report,
    );
    omitted(
        config,
        &[
            "thinking_summaries",
            "structured_speech_config",
            "video_config",
            "transcription_config",
        ],
        report,
    );
    Ok(out)
}

fn restore_call_signatures(
    steps: &mut Vec<Value>,
    report: &mut Report,
) -> Result<(), TransformError> {
    use base64::Engine;
    let mut signatures = std::collections::BTreeMap::new();
    steps.retain(|step| {
        if step["type"] == "thought"
            && let Some((id, signature)) = step["signature"]
                .as_str()
                .and_then(|value| value.strip_prefix("call:"))
                .and_then(|value| value.split_once(':'))
            && let Ok(id) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(id)
            && let Ok(id) = String::from_utf8(id)
        {
            signatures.insert(id, signature.to_owned());
            return false;
        }
        true
    });
    for step in steps {
        if step["type"] == "function_call"
            && let Some(signature) = signatures.remove(string(&step["id"]))
        {
            step["signature"] = json!(signature);
        }
    }
    if !signatures.is_empty() {
        report.omitted(
            "reasoning.signature",
            "tool signature carrier has no matching function call in the supplied history",
        );
    }
    Ok(())
}

// Interactions represents each function call as a step. Chat requires all
// parallel calls in the same assistant turn before any tool-result message.
fn merge_chat_assistants(messages: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for message in messages {
        if message["role"] == "assistant"
            && let Some(previous) = out
                .last_mut()
                .filter(|message| message["role"] == "assistant")
        {
            for field in ["content", "tool_calls"] {
                if let Some(parts) = message[field].as_array() {
                    let target = previous
                        .as_object_mut()
                        .expect("message object")
                        .entry(field)
                        .or_insert_with(|| json!([]));
                    target
                        .as_array_mut()
                        .expect("mapped message arrays")
                        .extend(parts.iter().cloned());
                }
            }
        } else {
            out.push(message);
        }
    }
    out
}

fn native_result(
    value: &Value,
    dialect: Dialect,
    report: &mut Report,
) -> Result<Value, TransformError> {
    if value.is_array() {
        Ok(json!(content::to_native(value, dialect, report)?))
    } else {
        Ok(value.clone())
    }
}

fn foreign_result(
    value: &Value,
    dialect: Dialect,
    report: &mut Report,
) -> Result<Value, TransformError> {
    if !value.is_array() {
        return Ok(json!(arguments(value)));
    }
    if dialect == Dialect::OpenAiChat {
        if array(value).any(|part| part["type"] != "text") {
            return Err(TransformError::unsupported(
                "function_result.media",
                "Chat tool results require text; media must be supplied in a user message",
            ));
        }
        return Ok(json!(text(value)));
    }
    Ok(json!(content::from_native(value, dialect, report)?))
}
