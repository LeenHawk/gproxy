//! OpenAI Images compatibility for DashScope's synchronous image endpoint.

use bytes::Bytes;
use serde_json::{Map, Value, json};

pub(super) fn create_request(body: Bytes) -> Bytes {
    let Ok(Value::Object(mut input)) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(prompt) = input.remove("prompt") else {
        return body;
    };

    let mut output = Map::new();
    if let Some(model) = input.remove("model") {
        output.insert("model".into(), model);
    }
    output.insert("input".into(), messages(vec![json!({ "text": prompt })]));
    output.insert("parameters".into(), parameters(&mut input));
    encode(Value::Object(output), body)
}

pub(super) fn edit_request(body: Bytes) -> Bytes {
    let Ok(Value::Object(mut input)) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(prompt) = input.remove("prompt") else {
        return body;
    };

    let mut content = Vec::new();
    let images = input
        .remove("images")
        .or_else(|| input.remove("image"))
        .map(image_values)
        .unwrap_or_default();
    for image in images {
        if let Some(url) = image_url(&image) {
            content.push(json!({ "image": url }));
        }
    }
    content.push(json!({ "text": prompt }));

    let mut output = Map::new();
    if let Some(model) = input.remove("model") {
        output.insert("model".into(), model);
    }
    output.insert("input".into(), messages(content));
    output.insert("parameters".into(), parameters(&mut input));
    encode(Value::Object(output), body)
}

fn messages(content: Vec<Value>) -> Value {
    json!({
        "messages": [{
            "role": "user",
            "content": content
        }]
    })
}

fn parameters(input: &mut Map<String, Value>) -> Value {
    let mut parameters = Map::new();
    if let Some(n) = input.remove("n") {
        parameters.insert("n".into(), n);
    }
    if let Some(mut size) = input.remove("size") {
        if let Some(value) = size.as_str() {
            size = Value::String(value.replace('x', "*"));
        }
        parameters.insert("size".into(), size);
    }
    parameters.insert("watermark".into(), Value::Bool(false));
    Value::Object(parameters)
}

fn image_values(value: Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values,
        value => vec![value],
    }
}

fn image_url(value: &Value) -> Option<&str> {
    match value {
        Value::String(value) => Some(value),
        Value::Object(value) => value.get("image_url").and_then(Value::as_str),
        _ => None,
    }
}

pub(super) fn image_response(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(contents) = value
        .pointer("/output/choices/0/message/content")
        .and_then(Value::as_array)
    else {
        return body;
    };
    let data = contents
        .iter()
        .filter_map(|content| content.get("image").and_then(Value::as_str))
        .map(|url| json!({ "url": url }))
        .collect::<Vec<_>>();
    if data.is_empty() {
        return body;
    }

    let mut output = Map::new();
    output.insert(
        "created".into(),
        Value::from(crate::util::time::unix_now().max(0) as u64),
    );
    output.insert("data".into(), Value::Array(data));
    if let Some(request_id) = value.get("request_id") {
        output.insert("request_id".into(), request_id.clone());
    }
    if let Some(usage) = value.get("usage") {
        output.insert("dashscope_usage".into(), usage.clone());
    }
    encode(Value::Object(output), body)
}

fn encode(value: Value, fallback: Bytes) -> Bytes {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_create_and_edit_requests() {
        let created = create_request(Bytes::from_static(
            br#"{"model":"qwen-image-3.0-pro","prompt":"cat","n":2,"size":"1024*1024"}"#,
        ));
        let created: Value = serde_json::from_slice(&created).unwrap();
        assert_eq!(created["input"]["messages"][0]["content"][0]["text"], "cat");
        assert_eq!(created["parameters"]["n"], 2);
        assert_eq!(created["parameters"]["size"], "1024*1024");

        let edited = edit_request(Bytes::from_static(
            br#"{"model":"wan2.7-image","prompt":"blue","images":[{"image_url":"https://example.com/a.png"},{"file_id":"file-1"}]}"#,
        ));
        let edited: Value = serde_json::from_slice(&edited).unwrap();
        assert_eq!(
            edited["input"]["messages"][0]["content"][0]["image"],
            "https://example.com/a.png"
        );
        assert_eq!(edited["input"]["messages"][0]["content"][1]["text"], "blue");
    }

    #[test]
    fn shapes_synchronous_image_response() {
        let shaped = image_response(Bytes::from_static(
            br#"{"output":{"choices":[{"message":{"content":[{"image":"https://example.com/out.png"}]}}]},"usage":{"image_count":1},"request_id":"req-1"}"#,
        ));
        let shaped: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(shaped["data"][0]["url"], "https://example.com/out.png");
        assert_eq!(shaped["request_id"], "req-1");
        assert!(shaped["created"].is_u64());
    }
}
