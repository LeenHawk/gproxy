use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::Operation;
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

mod policy;

pub(super) fn is_operation(operation: Operation) -> bool {
    matches!(operation, Operation::CreateImage | Operation::EditImage)
}

pub(super) fn request(
    operation: Operation,
    body: &Bytes,
    model: &str,
) -> Result<Bytes, ChannelError> {
    let model = required_model(model)?;
    let value = match operation {
        Operation::CreateImage => create(body, model)?,
        Operation::EditImage => edit(body, model)?,
        _ => {
            return Err(ChannelError::Prepare(
                "operation is not a DashScope image request".into(),
            ));
        }
    };
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(prepare_error)
}

fn create(body: &Bytes, model: &str) -> Result<Value, ChannelError> {
    let mut request: gproxy_protocol::openai::images::CreateImageRequest =
        serde_json::from_slice(body).map_err(prepare_error)?;
    request.model = Some(model.into());
    let Value::Object(mut fields) = serde_json::to_value(request).map_err(prepare_error)? else {
        return Err(ChannelError::Prepare(
            "image request is not an object".into(),
        ));
    };
    let model = fields.remove("model").expect("model was assigned");
    let prompt = fields
        .remove("prompt")
        .expect("prompt is required by schema");
    policy::drop_openai_only(&mut fields);
    Ok(envelope(model, vec![json!({"text":prompt})], fields))
}

fn edit(body: &Bytes, model: &str) -> Result<Value, ChannelError> {
    let normalized = normalize_edit_input(body)?;
    let mut request: gproxy_protocol::openai::images::EditImageRequest =
        serde_json::from_value(normalized).map_err(prepare_error)?;
    request.model = Some(model.into());
    let images = request
        .images
        .iter()
        .map(|image| {
            image
                .image_url
                .as_ref()
                .map(|url| json!({"image":url}))
                .ok_or_else(|| {
                    ChannelError::Prepare("DashScope image edit requires image_url".into())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if images.is_empty() {
        return Err(ChannelError::Prepare(
            "DashScope image edit requires an image".into(),
        ));
    }
    let Value::Object(mut fields) = serde_json::to_value(request).map_err(prepare_error)? else {
        return Err(ChannelError::Prepare(
            "image request is not an object".into(),
        ));
    };
    let model = fields.remove("model").expect("model was assigned");
    let prompt = fields
        .remove("prompt")
        .expect("prompt is required by schema");
    fields.remove("images");
    policy::drop_openai_only(&mut fields);
    let mut content = images;
    content.push(json!({"text":prompt}));
    Ok(envelope(model, content, fields))
}

fn normalize_edit_input(body: &Bytes) -> Result<Value, ChannelError> {
    let mut value: Value = serde_json::from_slice(body).map_err(prepare_error)?;
    let root = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("image request must be an object".into()))?;
    if !root.contains_key("images")
        && let Some(image) = root.remove("image")
    {
        root.insert("images".into(), Value::Array(vec![image]));
    }
    if let Some(images) = root.get_mut("images").and_then(Value::as_array_mut) {
        for image in images {
            if let Value::String(url) = image {
                *image = json!({"image_url":std::mem::take(url)});
            }
        }
    }
    Ok(value)
}

fn envelope(model: Value, content: Vec<Value>, mut parameters: Map<String, Value>) -> Value {
    if let Some(Value::String(size)) = parameters.get_mut("size") {
        *size = size.replace('x', "*");
    }
    parameters
        .entry("watermark")
        .or_insert_with(|| Value::Bool(false));
    json!({
        "model":model,
        "input":{"messages":[{"role":"user","content":content}]},
        "parameters":parameters
    })
}

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body).map_err(observe_error)?;
    let contents = value
        .pointer("/output/choices/0/message/content")
        .and_then(Value::as_array)
        .ok_or_else(|| ChannelError::Observe("DashScope image response has no content".into()))?;
    let data = contents
        .iter()
        .filter_map(|content| content.get("image").and_then(Value::as_str))
        .map(|url| json!({"url":url}))
        .collect::<Vec<_>>();
    if data.is_empty() {
        return Err(ChannelError::Observe(
            "DashScope image response has no image URL".into(),
        ));
    }
    let mut output = Map::new();
    output.insert("data".into(), Value::Array(data));
    if let Some(request_id) = value.get("request_id") {
        output.insert("request_id".into(), request_id.clone());
    }
    if let Some(usage) = value.get("usage") {
        output.insert("dashscope_usage".into(), usage.clone());
    }
    serde_json::to_vec(&Value::Object(output))
        .map(Bytes::from)
        .map_err(observe_error)
}

pub(super) fn usage(body: &[u8]) -> Option<gproxy_channel_api::NormalizedUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let raw = value.get("usage")?.as_object()?;
    let mut usage = gproxy_channel_api::NormalizedUsage::default();
    let mut measured = false;
    if let Some(tokens) = raw.get("input_tokens").and_then(Value::as_u64) {
        usage.input_tokens = tokens;
        measured = true;
    }
    if let Some(tokens) = raw.get("output_tokens").and_then(Value::as_u64) {
        usage.output_tokens = tokens;
        measured = true;
    }
    if let Some(outputs) = raw.get("image_count").and_then(Value::as_u64) {
        usage
            .metrics
            .insert("image_outputs".into(), Decimal::from(outputs));
        measured = true;
    }
    if let Some(size) = raw
        .get("size")
        .or_else(|| value.get("size"))
        .and_then(Value::as_str)
    {
        usage.dimensions.insert("size".into(), size.into());
        measured = true;
    }
    measured.then_some(usage)
}

fn required_model(model: &str) -> Result<&str, ChannelError> {
    (!model.trim().is_empty())
        .then_some(model.trim())
        .ok_or_else(|| ChannelError::Prepare("DashScope image request has no model".into()))
}

fn prepare_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("DashScope image JSON: {error}"))
}

fn observe_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(format!("DashScope image JSON: {error}"))
}
