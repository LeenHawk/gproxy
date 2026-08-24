use gproxy_channel_api::ChannelError;
use serde_json::{Value, json};

pub(super) fn result(item: &Value) -> Result<Value, ChannelError> {
    let output = item
        .get("output")
        .expect("typed function-call output has output");
    let output = output
        .as_str()
        .map_or_else(|| output.to_string(), str::to_owned);
    Ok(json!({
        "toolUseId":call_id(item)?,
        "content":[{"text":output}],
        "status":"success"
    }))
}

pub(super) fn append_call(messages: &mut Vec<Value>, item: &Value) -> Result<(), ChannelError> {
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .expect("typed function call has arguments");
    let input: Value = serde_json::from_str(arguments)
        .map_err(|error| ChannelError::Prepare(format!("Kiro tool arguments JSON: {error}")))?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ChannelError::Prepare("Kiro tool call name is empty".into()))?;
    let entry = json!({
        "toolUseId":call_id(item)?,
        "name":name,
        "input":input
    });
    if let Some(message) = messages
        .last_mut()
        .and_then(|message| message.get_mut("assistantResponseMessage"))
        .and_then(Value::as_object_mut)
    {
        message
            .entry("toolUses")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("toolUses is an array")
            .push(entry);
    } else {
        let mut message = super::content::assistant(".".into());
        message["assistantResponseMessage"]["toolUses"] = Value::Array(vec![entry]);
        messages.push(message);
    }
    Ok(())
}

pub(super) fn attach_results(message: &mut Value, results: Vec<Value>) {
    if results.is_empty() {
        return;
    }
    message["userInputMessage"]["userInputMessageContext"]["toolResults"] = Value::Array(results);
}

pub(super) fn flush_results(messages: &mut Vec<Value>, results: &mut Vec<Value>, model: &str) {
    if results.is_empty() {
        return;
    }
    let mut message = super::content::user(".", model, Vec::new());
    attach_results(&mut message, std::mem::take(results));
    messages.push(message);
}

pub(super) fn definitions(tools: Option<&Value>) -> Result<Vec<Value>, ChannelError> {
    let Some(tools) = tools.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    tools
        .iter()
        .filter_map(|tool| {
            if tool
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "function")
            {
                return None;
            }
            let function = tool.get("function").unwrap_or(tool);
            let name = function.get("name")?.as_str()?;
            let description = function
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(name)
                .chars()
                .take(10_237)
                .collect::<String>();
            let mut schema = function
                .get("parameters")
                .or_else(|| function.get("input_schema"))
                .cloned()
                .unwrap_or_else(|| json!({"type":"object"}));
            if !schema.is_object() {
                schema = json!({"type":"object"});
            }
            clean_schema(&mut schema);
            if let Some(schema) = schema.as_object_mut() {
                schema
                    .entry("type")
                    .or_insert_with(|| Value::String("object".into()));
            }
            Some(Ok(json!({"toolSpecification":{
                "name":sanitize(name),
                "description":description,
                "inputSchema":{"json":schema}
            }})))
        })
        .collect()
}

pub(super) fn attach_definitions(current: &mut Value, tools: Vec<Value>) {
    if !tools.is_empty() {
        current["userInputMessage"]["userInputMessageContext"]["tools"] = Value::Array(tools);
    }
}

fn clean_schema(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("additionalProperties");
            if object
                .get("required")
                .is_some_and(|value| value.as_array().is_none_or(Vec::is_empty))
            {
                object.remove("required");
            }
            for value in object.values_mut() {
                clean_schema(value);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(clean_schema),
        _ => {}
    }
}

fn sanitize(name: &str) -> String {
    let mut output = String::new();
    for (index, part) in name
        .split(['_', '-', ' ', '.', '/', ':'])
        .filter(|part| !part.is_empty())
        .enumerate()
    {
        let mut chars = part.chars().filter(char::is_ascii_alphanumeric);
        if let Some(first) = chars.next() {
            output.push(if index == 0 {
                first.to_ascii_lowercase()
            } else {
                first.to_ascii_uppercase()
            });
            output.extend(chars);
        }
    }
    if output.is_empty() {
        "tool".into()
    } else {
        output.chars().take(64).collect()
    }
}

fn call_id(item: &Value) -> Result<&str, ChannelError> {
    item.get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ChannelError::Prepare("Kiro tool call id is empty".into()))
}
