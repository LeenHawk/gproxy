//! Current MakerSuite JSPB fields from the first-party bundle and browser
//! captures. In particular, Part.functionCall/functionResponse are fields
//! 11/12, thought is field 13, and thoughtSignature is field 15.

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use serde_json::{Map, Value, json};

use super::{request::invalid, schema::lower};
use crate::channel::ChannelError;

fn set(array: &mut Vec<Value>, index: usize, value: Value) {
    if !value.is_null() {
        array.resize(array.len().max(index + 1), Value::Null);
        array[index] = value;
    }
}

/// A request field under its JSON name, or under the proto field name
/// (`system_instruction`) that Google's API accepts as well.
fn key<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    value.get(name).or_else(|| {
        if !name.bytes().any(|c| c.is_ascii_uppercase()) {
            return None;
        }
        let mut snake = String::with_capacity(name.len() + 4);
        for c in name.chars() {
            if c.is_ascii_uppercase() {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        }
        value.get(&snake)
    })
}

fn field(array: &mut Vec<Value>, index: usize, value: &Value, name: &str) {
    if let Some(value) = key(value, name) {
        set(array, index, value.clone());
    }
}

fn bad_request(message: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!("Playground request: {message}"))
}

fn base64(value: &Value) -> Result<Value, ChannelError> {
    let text = value
        .as_str()
        .ok_or_else(|| bad_request("expected base64 string"))?;
    let bytes = STANDARD
        .decode(text)
        .or_else(|_| STANDARD_NO_PAD.decode(text))
        .or_else(|_| URL_SAFE.decode(text))
        .or_else(|_| URL_SAFE_NO_PAD.decode(text))
        .map_err(|_| bad_request("invalid base64 data"))?;
    Ok(STANDARD.encode(bytes).into())
}

fn struct_value(value: &Value) -> Value {
    match value {
        Value::Null => json!([0]),
        Value::Bool(v) => json!([null, null, null, v]),
        Value::Number(v) => json!([null, v]),
        Value::String(v) => json!([null, null, v]),
        Value::Array(values) => json!([
            null,
            null,
            null,
            null,
            null,
            [values.iter().map(struct_value).collect::<Vec<_>>()]
        ]),
        Value::Object(_) => json!([null, null, null, null, encode_struct(value)]),
    }
}

fn encode_struct(value: &Value) -> Value {
    json!([value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, value)| json!([key, struct_value(value)]))
        .collect::<Vec<_>>()])
}

fn decode_value(value: &Value) -> Result<Value, ChannelError> {
    let values = value
        .as_array()
        .ok_or_else(|| invalid("invalid protobuf Value"))?;
    for (index, value) in values.iter().enumerate() {
        if value.is_null() {
            continue;
        }
        return match index {
            0 => Ok(Value::Null),
            1..=3 => Ok(value.clone()),
            4 => decode_struct(value),
            5 => Ok(Value::Array(
                value
                    .get(0)
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid("invalid protobuf ListValue"))?
                    .iter()
                    .map(decode_value)
                    .collect::<Result<_, _>>()?,
            )),
            _ => Err(invalid("unknown protobuf Value field")),
        };
    }
    Ok(Value::Null)
}

fn decode_struct(value: &Value) -> Result<Value, ChannelError> {
    let mut object = Map::new();
    if let Some(entries) = value.get(0).and_then(Value::as_array) {
        for entry in entries {
            let key = entry
                .get(0)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("invalid protobuf Struct key"))?;
            let value = entry
                .get(1)
                .ok_or_else(|| invalid("missing protobuf Struct value"))?;
            object.insert(key.into(), decode_value(value)?);
        }
    }
    Ok(object.into())
}

fn schema(value: &Value) -> Result<Value, ChannelError> {
    let mut out = vec![];
    if let Some(kind) = value.get("type").and_then(Value::as_str) {
        let kind = match kind.to_ascii_uppercase().as_str() {
            "STRING" => 1,
            "NUMBER" => 2,
            "INTEGER" => 3,
            "BOOLEAN" => 4,
            "ARRAY" => 5,
            "OBJECT" => 6,
            "NULL" => 7,
            _ => return Err(bad_request("unsupported schema type")),
        };
        set(&mut out, 0, kind.into());
    }
    for (name, index) in [
        ("format", 1),
        ("description", 2),
        ("nullable", 3),
        ("enum", 4),
        ("required", 7),
        ("minProperties", 8),
        ("maxProperties", 9),
        ("minimum", 10),
        ("maximum", 11),
        ("minLength", 12),
        ("maxLength", 13),
        ("pattern", 14),
        ("minItems", 21),
        ("maxItems", 20),
        ("propertyOrdering", 22),
    ] {
        field(&mut out, index, value, name);
    }
    if let Some(items) = value.get("items") {
        set(&mut out, 5, schema(items)?);
    }
    if let Some(properties) = value.get("properties").and_then(Value::as_object) {
        set(
            &mut out,
            6,
            Value::Array(
                properties
                    .iter()
                    .map(|(key, value)| Ok(json!([key, schema(value)?])))
                    .collect::<Result<_, ChannelError>>()?,
            ),
        );
    }
    for (name, index) in [("oneOf", 16), ("anyOf", 17), ("allOf", 18)] {
        if let Some(values) = value.get(name).and_then(Value::as_array) {
            set(
                &mut out,
                index,
                Value::Array(values.iter().map(schema).collect::<Result<_, _>>()?),
            );
        }
    }
    if let Some(value) = value.get("not") {
        set(&mut out, 19, schema(value)?);
    }
    if let Some(value) = value.get("example") {
        set(&mut out, 15, struct_value(value));
    }
    if value.get("$ref").is_some() {
        return Err(bad_request(
            "schema references must be resolved before sending",
        ));
    }
    Ok(out.into())
}

fn part(value: &Value) -> Result<Value, ChannelError> {
    let mut out = vec![];
    field(&mut out, 1, value, "text");
    if let Some(data) = key(value, "inlineData") {
        let mime = key(data, "mimeType")
            .and_then(Value::as_str)
            .ok_or_else(|| bad_request("inlineData requires mimeType"))?;
        set(&mut out, 2, json!([mime, base64(&data["data"])?]));
    }
    for (name, index, body) in [
        ("functionCall", 10, "args"),
        ("functionResponse", 11, "response"),
    ] {
        if let Some(call) = key(value, name) {
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| bad_request("function part requires name"))?;
            let body = call.get(body).cloned().unwrap_or_else(|| json!({}));
            if !body.is_object() {
                return Err(bad_request(
                    "function arguments and responses must be objects",
                ));
            }
            let mut call_wire = vec![name.into(), encode_struct(&body)];
            field(&mut call_wire, 2, call, "id");
            set(&mut out, index, call_wire.into());
        }
    }
    if key(value, "fileData").is_some() {
        return Err(bad_request("fileData is not supported; use inlineData"));
    }
    if let Some(code) = key(value, "executableCode") {
        if code["language"].as_str() != Some("PYTHON") {
            return Err(bad_request("unsupported executableCode language"));
        }
        set(&mut out, 7, json!([1, code["code"]]));
    }
    if let Some(result) = key(value, "codeExecutionResult") {
        let outcome = match result["outcome"].as_str() {
            Some("OUTCOME_OK") => 1,
            Some("OUTCOME_FAILED") => 2,
            Some("OUTCOME_DEADLINE_EXCEEDED") => 3,
            _ => return Err(bad_request("unsupported codeExecutionResult outcome")),
        };
        set(&mut out, 8, json!([outcome, result["output"]]));
    }
    field(&mut out, 12, value, "thought");
    if let Some(signature) = key(value, "thoughtSignature") {
        set(&mut out, 14, base64(signature)?);
    }
    if out.is_empty() {
        return Err(bad_request("empty or unsupported content part"));
    }
    Ok(out.into())
}

fn content(value: &Value, default_role: &str) -> Result<Value, ChannelError> {
    let parts = value
        .get("parts")
        .and_then(Value::as_array)
        .ok_or_else(|| bad_request("content requires parts"))?;
    Ok(json!([
        parts.iter().map(part).collect::<Result<Vec<_>, _>>()?,
        value
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or(default_role)
    ]))
}

fn generation_config(value: &Value) -> Result<Value, ChannelError> {
    if key(value, "candidateCount")
        .is_some_and(|count| count.as_u64() != Some(1))
    {
        return Err(bad_request(
            "Playground currently supports one candidate per request",
        ));
    }
    let mut out = vec![];
    for (name, index) in [
        ("stopSequences", 1),
        ("maxOutputTokens", 3),
        ("temperature", 4),
        ("topP", 5),
        ("topK", 6),
        ("responseMimeType", 7),
        ("presencePenalty", 9),
        ("frequencyPenalty", 10),
        ("responseLogprobs", 11),
        ("logprobs", 12),
        ("seed", 18),
    ] {
        field(&mut out, index, value, name);
    }
    // The wire has one schema slot; a JSON Schema is lowered into it.
    if let Some(json_schema) = key(value, "responseJsonSchema")
        .or_else(|| key(value, "_responseJsonSchema"))
    {
        set(&mut out, 8, schema(&lower(json_schema))?);
    } else if let Some(schema_value) = key(value, "responseSchema") {
        set(&mut out, 8, schema(schema_value)?);
    }
    if let Some(modalities) = key(value, "responseModalities").and_then(Value::as_array) {
        let modes = modalities
            .iter()
            .map(|value| match value.as_str() {
                Some("TEXT") => Ok(json!(1)),
                Some("IMAGE") => Ok(json!(2)),
                Some("AUDIO") => Ok(json!(3)),
                _ => Err(bad_request("unsupported response modality")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        set(&mut out, 14, modes.into());
    }
    if let Some(thinking) = key(value, "thinkingConfig") {
        let mut config = vec![];
        if let Some(include) = key(thinking, "includeThoughts").and_then(Value::as_bool) {
            set(&mut config, 0, u8::from(include).into());
        }
        field(&mut config, 1, thinking, "thinkingBudget");
        if let Some(level) = key(thinking, "thinkingLevel").and_then(Value::as_str) {
            let level = match level.to_ascii_uppercase().as_str() {
                "MINIMAL" => 4,
                "LOW" => 1,
                "HIGH" => 3,
                "MEDIUM" => 2,
                _ => return Err(bad_request("unsupported thinking level")),
            };
            set(&mut config, 3, level.into());
        }
        set(&mut out, 16, config.into());
    }
    if let Some(image) = key(value, "imageConfig") {
        let mut config = vec![];
        field(&mut config, 0, image, "aspectRatio");
        field(&mut config, 1, image, "imageSize");
        set(&mut out, 26, config.into());
    }
    for name in ["speechConfig", "videoConfig", "mediaResolution"] {
        if key(value, name).is_some() {
            return Err(bad_request(
                "speechConfig, videoConfig and mediaResolution are not supported yet",
            ));
        }
    }
    Ok(out.into())
}

pub(super) fn encode(model: &str, value: &Value, timezone: &str) -> Result<Value, ChannelError> {
    let contents = value
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(|| bad_request("contents must be an array"))?;
    let mut out = vec![
        Value::String(model.into()),
        Value::Array(
            contents
                .iter()
                .map(|v| content(v, "user"))
                .collect::<Result<_, _>>()?,
        ),
    ];
    if let Some(settings) = key(value, "safetySettings").and_then(Value::as_array) {
        let safety = settings
            .iter()
            .map(|setting| {
                let category = match setting["category"].as_str() {
                    Some("HARM_CATEGORY_HARASSMENT") => 7,
                    Some("HARM_CATEGORY_HATE_SPEECH") => 8,
                    Some("HARM_CATEGORY_SEXUALLY_EXPLICIT") => 9,
                    Some("HARM_CATEGORY_DANGEROUS_CONTENT") => 10,
                    Some("HARM_CATEGORY_CIVIC_INTEGRITY") => 11,
                    _ => return Err(bad_request("unsupported safety category")),
                };
                let threshold = match setting["threshold"].as_str() {
                    Some("HARM_BLOCK_THRESHOLD_UNSPECIFIED") => 0,
                    Some("BLOCK_LOW_AND_ABOVE") => 1,
                    Some("BLOCK_MEDIUM_AND_ABOVE") => 2,
                    Some("BLOCK_ONLY_HIGH") => 3,
                    Some("BLOCK_NONE") => 4,
                    Some("OFF") => 5,
                    _ => return Err(bad_request("unsupported safety threshold")),
                };
                Ok(json!([null, null, category, threshold]))
            })
            .collect::<Result<Vec<_>, _>>()?;
        set(&mut out, 2, safety.into());
    }
    if let Some(config) = key(value, "generationConfig") {
        set(&mut out, 3, generation_config(config)?);
    }
    if let Some(system) = key(value, "systemInstruction") {
        set(&mut out, 5, content(system, "user")?);
    }
    // The calling mode has no known slot on this wire. `NONE` is honoured by
    // leaving the declarations out; every other mode runs as the default
    // `AUTO`, so a forced call is only a request the model may decline.
    let no_calls = key(value, "toolConfig")
        .and_then(|config| key(config, "functionCallingConfig"))
        .and_then(|config| config.get("mode"))
        .and_then(Value::as_str)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("NONE"));
    if let Some(tools) = value.get("tools").and_then(Value::as_array) {
        let mut encoded = vec![];
        for tool in tools {
            let mut item = vec![];
            let declarations = key(tool, "functionDeclarations").and_then(Value::as_array);
            if let Some(declarations) = declarations.filter(|_| !no_calls) {
                let functions = declarations
                    .iter()
                    .map(|declaration| {
                        let name = declaration
                            .get("name")
                            .and_then(Value::as_str)
                            .ok_or_else(|| bad_request("function declaration requires name"))?;
                        let mut function = vec![name.into()];
                        field(&mut function, 1, declaration, "description");
                        if let Some(json_schema) = key(declaration, "parametersJsonSchema") {
                            set(&mut function, 2, schema(&lower(json_schema))?);
                        } else if let Some(parameters) = declaration.get("parameters") {
                            set(&mut function, 2, schema(parameters)?);
                        }
                        Ok::<_, ChannelError>(Value::Array(function))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                set(&mut item, 1, functions.into());
            }
            for (name, index) in [("codeExecution", 0), ("urlContext", 7), ("googleMaps", 10)] {
                if key(tool, name).is_some() {
                    set(&mut item, index, json!([]));
                }
            }
            // AI Studio has one search tool; the older retrieval tool of the
            // public API selects it too, without its dynamic threshold. An
            // image model may ask for image search with or instead of web.
            if let Some(search) =
                key(tool, "googleSearch").or_else(|| key(tool, "googleSearchRetrieval"))
            {
                let types = key(search, "searchTypes");
                let asks = |name| types.and_then(|types| key(types, name)).is_some();
                let kinds = match (asks("webSearch"), asks("imageSearch")) {
                    (true, true) => json!([[], []]),
                    (false, true) => json!([null, []]),
                    _ => json!([[]]),
                };
                set(&mut item, 3, json!([null, kinds]));
            }
            if item.is_empty() {
                if no_calls && declarations.is_some() {
                    continue;
                }
                let names = tool
                    .as_object()
                    .map(|tool| tool.keys().map(String::as_str).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();
                return Err(bad_request(&format!("unsupported tool `{names}`")));
            }
            encoded.push(item);
        }
        if !encoded.is_empty() {
            set(&mut out, 6, json!(encoded));
        }
    }
    if key(value, "cachedContent").is_some() {
        return Err(bad_request("cachedContent is not supported"));
    }
    set(&mut out, 13, json!([[null, null, timezone]]));
    Ok(out.into())
}

pub(super) fn prompt(wire: &Value) -> String {
    wire.get(1)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|content| {
            content
                .get(0)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .map(|part| {
            part.get(2)
                .and_then(|data| data.get(1))
                .and_then(Value::as_str)
                .or_else(|| part.get(1).and_then(Value::as_str))
                .unwrap_or("")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_part(part: &Value) -> Result<Value, ChannelError> {
    let mut out = Map::new();
    if let Some(text) = part.get(1).and_then(Value::as_str) {
        out.insert("text".into(), text.into());
    }
    if let Some(data) = part.get(2).filter(|v| v.is_array()) {
        out.insert(
            "inlineData".into(),
            json!({"mimeType":data[0],"data":base64(&data[1])?}),
        );
    }
    for (index, name, key) in [
        (10, "functionCall", "args"),
        (11, "functionResponse", "response"),
    ] {
        if let Some(call) = part.get(index).filter(|v| v.is_array()) {
            let mut object = Map::new();
            object.insert("name".into(), call[0].clone());
            object.insert(key.into(), decode_struct(&call[1])?);
            if let Some(id) = call.get(2).and_then(Value::as_str) {
                object.insert("id".into(), id.into());
            }
            out.insert(name.into(), object.into());
        }
    }
    if let Some(code) = part.get(7).filter(|v| v.is_array()) {
        out.insert(
            "executableCode".into(),
            json!({"language":"PYTHON","code":code[1]}),
        );
    }
    if let Some(result) = part.get(8).filter(|v| v.is_array()) {
        let outcome = match result[0].as_u64() {
            Some(1) => "OUTCOME_OK",
            Some(2) => "OUTCOME_FAILED",
            Some(3) => "OUTCOME_DEADLINE_EXCEEDED",
            _ => "OUTCOME_UNSPECIFIED",
        };
        out.insert(
            "codeExecutionResult".into(),
            json!({"outcome":outcome,"output":result[1]}),
        );
    }
    if part.get(12).is_some_and(|v| v == true || v == 1) {
        out.insert("thought".into(), true.into());
    }
    if let Some(signature) = part.get(14).filter(|v| !v.is_null()) {
        out.insert("thoughtSignature".into(), base64(signature)?);
    }
    if out.is_empty() {
        return Err(invalid("unsupported response part"));
    }
    Ok(out.into())
}

/// `None` for an element that carries no content, such as the timing trailer
/// `[null,null,null,["<micros>",n,n]]` Google appends after the last chunk.
pub(super) fn response(value: &Value, model: &str) -> Result<Option<Value>, ChannelError> {
    let mut out = Map::new();
    out.insert(
        "modelVersion".into(),
        model.trim_start_matches("models/").into(),
    );
    if let Some(candidates) = value.get(0).and_then(Value::as_array) {
        let candidates = candidates.iter().enumerate().map(|(index,candidate)| {
            let mut out = Map::new();
            out.insert("index".into(),index.into());
            if let Some(content) = candidate.get(0).filter(|v| v.is_array()) {
                let parts = content.get(0).and_then(Value::as_array).ok_or_else(|| invalid("missing response parts"))?;
                out.insert("content".into(),json!({"role":content.get(1).and_then(Value::as_str).unwrap_or("model"),"parts":parts.iter().map(decode_part).collect::<Result<Vec<_>,_>>()?}));
            }
            if let Some(reason) = candidate.get(1).and_then(Value::as_u64).filter(|v| *v != 0 && *v != 22) {
                let reason = match reason { 1=>"STOP",2=>"MAX_TOKENS",3=>"SAFETY",4=>"RECITATION",6=>"BLOCKLIST",7=>"PROHIBITED_CONTENT",8=>"SPII",9=>"MALFORMED_FUNCTION_CALL",_=>"OTHER" };
                out.insert("finishReason".into(),reason.into());
            }
            Ok(Value::Object(out))
        }).collect::<Result<Vec<_>,ChannelError>>()?;
        out.insert("candidates".into(), candidates.into());
    }
    if let Some(usage) = value.get(2).and_then(Value::as_array) {
        let mut normalized = Map::new();
        for (index, key) in [
            (0, "promptTokenCount"),
            (1, "candidatesTokenCount"),
            (2, "totalTokenCount"),
            (3, "cachedContentTokenCount"),
            (9, "thoughtsTokenCount"),
        ] {
            if let Some(value) = usage.get(index).filter(|v| v.is_number()) {
                normalized.insert(key.into(), value.clone());
            }
        }
        if !normalized.is_empty() {
            out.insert("usageMetadata".into(), normalized.into());
        }
    }
    if let Some(id) = value.get(7).and_then(Value::as_str) {
        out.insert("responseId".into(), id.into());
    }
    if !out.contains_key("candidates") && !out.contains_key("usageMetadata") {
        return Ok(None);
    }
    Ok(Some(out.into()))
}

pub(super) fn models(value: &Value) -> Result<Vec<Value>, ChannelError> {
    let models = value
        .get(0)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("invalid ListModels response"))?;
    models
        .iter()
        .filter(|model| {
            model
                .get(7)
                .and_then(Value::as_array)
                .is_some_and(|methods| methods.iter().any(|method| method == "generateContent"))
        })
        .map(|model| {
            let name = model
                .get(0)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("missing model name"))?;
            let mut out = Map::new();
            out.insert("name".into(), name.into());
            for (index, key) in [
                (2, "version"),
                (3, "displayName"),
                (4, "description"),
                (5, "inputTokenLimit"),
                (6, "outputTokenLimit"),
                (8, "temperature"),
                (9, "topP"),
                (10, "topK"),
            ] {
                if let Some(value) = model.get(index).filter(|v| !v.is_null()) {
                    out.insert(key.into(), value.clone());
                }
            }
            out.insert(
                "supportedGenerationMethods".into(),
                json!(["generateContent", "countTokens"]),
            );
            Ok(out.into())
        })
        .collect()
}
