use bytes::Bytes;
use serde_json::{Value, json};

use crate::channel::ShapeCtx;
use crate::protocol::Operation;

pub(super) fn request(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    if !matches!(
        ctx.op.operation(),
        Operation::CreateImage | Operation::EditImage
    ) {
        return body;
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(object) = value.as_object_mut() else {
        return body;
    };
    let allowed: &[&str] = match ctx.op.operation() {
        Operation::CreateImage => &[
            "prompt",
            "background",
            "model",
            "n",
            "quality",
            "response_format",
            "size",
            "style",
            "footnote",
            "revise",
        ],
        Operation::EditImage => &[
            "image",
            "images",
            "prompt",
            "background",
            "input_fidelity",
            "model",
            "n",
            "quality",
            "response_format",
            "size",
            "style",
            "footnote",
            "revise",
        ],
        _ => &[],
    };
    object.retain(|key, _| allowed.contains(&key.as_str()));
    object
        .entry("response_format")
        .or_insert_with(|| Value::String("b64_json".into()));
    match ctx.op.operation() {
        Operation::CreateImage => {
            object
                .entry("model")
                .or_insert_with(|| Value::String("hunyuan-image-v3.0".into()));
        }
        Operation::EditImage => {
            object
                .entry("model")
                .or_insert_with(|| Value::String("hunyuan-image-v2.0-general-edit".into()));
            if let Some(images) = object.remove("images").or_else(|| object.remove("image")) {
                object.insert("image".into(), normalize_images(images));
            }
        }
        _ => {}
    }
    serde_json::to_vec(&value).map(Bytes::from).unwrap_or(body)
}

pub(super) fn response(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    if !ctx.status.is_success() {
        return body;
    }
    match ctx.op.operation() {
        Operation::CreateImage | Operation::EditImage => unwrap_images(body),
        Operation::ListModels => model_list(body),
        _ => body,
    }
}

fn unwrap_images(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(code) = value.get("code").and_then(Value::as_i64) else {
        return body;
    };
    if code != 0 {
        return body;
    }
    let Some(mut data) = value.get("data").cloned() else {
        return body;
    };
    if let Some(object) = data.as_object_mut()
        && object.get("data").is_some_and(Value::is_array)
    {
        object
            .entry("created")
            .or_insert_with(|| json!(crate::util::time::unix_now().max(0)));
    }
    serde_json::to_vec(&data).map(Bytes::from).unwrap_or(body)
}

fn model_list(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(models) = value.get("data").and_then(|data| data.get("models")) else {
        return body;
    };
    let Some(models) = models.as_array() else {
        return Bytes::from_static(br#"{"object":"list","data":[]}"#);
    };
    let models = models
        .iter()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": "tencent"
            })
        })
        .collect::<Vec<_>>();
    Bytes::from(json!({ "object": "list", "data": models }).to_string())
}

fn normalize_images(value: Value) -> Value {
    let values = match value {
        Value::Array(values) => values,
        value => vec![value],
    };
    Value::Array(
        values
            .into_iter()
            .map(|value| match value {
                Value::Object(object) => object
                    .get("image_url")
                    .or_else(|| object.get("url"))
                    .cloned()
                    .map(normalize_image_string)
                    .unwrap_or(Value::Object(object)),
                Value::String(value) => normalize_image_string(Value::String(value)),
                value => value,
            })
            .collect(),
    )
}

fn normalize_image_string(value: Value) -> Value {
    let Value::String(value) = value else {
        return value;
    };
    Value::String(value.strip_prefix("data:").unwrap_or(&value).to_string())
}
