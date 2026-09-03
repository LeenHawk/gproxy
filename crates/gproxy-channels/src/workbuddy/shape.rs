use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::Operation;
use http::header::{CONTENT_TYPE, HeaderValue};
use serde_json::Value;

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut http::HeaderMap,
) -> Result<Bytes, ChannelError> {
    if ctx.stream {
        return Err(ChannelError::Prepare(
            "WorkBuddy image operations do not stream".into(),
        ));
    }
    let mut object = crate::shared::image_multipart::object(ctx.headers, ctx.body)?;
    crate::shared::image_multipart::json_fields(&mut object, &["footnote", "n", "revise"]);
    let allowed = match ctx.key.operation() {
        Operation::CreateImage => CREATE_FIELDS,
        Operation::EditImage => EDIT_FIELDS,
        _ => {
            return Err(ChannelError::Prepare(
                "operation is not a WorkBuddy image request".into(),
            ));
        }
    };
    object.retain(|name, _| allowed.contains(&name.as_str()));
    object
        .entry("response_format")
        .or_insert_with(|| Value::String("b64_json".into()));
    let fallback = if ctx.key.operation() == Operation::CreateImage {
        "hunyuan-image-v3.0"
    } else {
        "hunyuan-image-v2.0-general-edit"
    };
    object.insert(
        "model".into(),
        Value::String(if ctx.upstream_model.trim().is_empty() {
            fallback.into()
        } else {
            ctx.upstream_model.into()
        }),
    );
    if ctx.key.operation() == Operation::EditImage {
        let images = object
            .remove("images")
            .or_else(|| object.remove("image"))
            .ok_or_else(|| ChannelError::Prepare("WorkBuddy image edit has no image".into()))?;
        object.insert("image".into(), normalize_images(images));
    }
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    serde_json::to_vec(&Value::Object(object))
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn response(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if !ctx.status.is_success() {
        return Ok(ctx.body.clone());
    }
    match ctx.key.operation() {
        Operation::ListModels => model_list(ctx.body),
        Operation::CreateImage | Operation::EditImage => image_response(ctx.body),
        _ => Ok(ctx.body.clone()),
    }
}

const CREATE_FIELDS: &[&str] = &[
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
];

const EDIT_FIELDS: &[&str] = &[
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
];

fn normalize_images(value: Value) -> Value {
    let images = match value {
        Value::Array(images) => images,
        image => vec![image],
    };
    Value::Array(
        images
            .into_iter()
            .map(|image| match image {
                Value::Object(object) => object
                    .get("image_url")
                    .or_else(|| object.get("url"))
                    .cloned()
                    .map(normalize_image)
                    .unwrap_or(Value::Object(object)),
                image => normalize_image(image),
            })
            .collect(),
    )
}

fn normalize_image(value: Value) -> Value {
    let Value::String(value) = value else {
        return value;
    };
    Value::String(value.strip_prefix("data:").unwrap_or(&value).into())
}

fn image_response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("WorkBuddy image JSON: {error}")))?;
    if value.get("code").and_then(Value::as_i64) != Some(0) {
        return Ok(body.clone());
    }
    let Value::Object(mut outer) = value else {
        return Err(ChannelError::Observe(
            "WorkBuddy image response is not an object".into(),
        ));
    };
    let Value::Object(mut inner) = outer
        .remove("data")
        .ok_or_else(|| ChannelError::Observe("WorkBuddy image response missing data".into()))?
    else {
        return Err(ChannelError::Observe(
            "WorkBuddy image response data is not an object".into(),
        ));
    };
    for (name, value) in outer {
        inner.entry(name).or_insert(value);
    }
    Ok(Bytes::from(Value::Object(inner).to_string()))
}

fn model_list(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("WorkBuddy model JSON: {error}")))?;
    let Value::Object(mut outer) = value else {
        return Err(ChannelError::Observe(
            "WorkBuddy model response is not an object".into(),
        ));
    };
    if outer.get("code").and_then(Value::as_i64) != Some(0) {
        return Ok(body.clone());
    }
    let Value::Object(mut inner) = outer
        .remove("data")
        .ok_or_else(|| ChannelError::Observe("WorkBuddy model response missing data".into()))?
    else {
        return Err(ChannelError::Observe(
            "WorkBuddy model response data is not an object".into(),
        ));
    };
    let models = inner
        .remove("models")
        .and_then(|models| models.as_array().cloned())
        .ok_or_else(|| ChannelError::Observe("WorkBuddy model response missing models".into()))?;
    let data = models
        .into_iter()
        .filter_map(|model| {
            let Value::Object(mut model) = model else {
                return None;
            };
            model.get("id")?.as_str()?;
            model
                .entry("object")
                .or_insert_with(|| Value::String("model".into()));
            Some(Value::Object(model))
        })
        .collect::<Vec<_>>();
    let mut output = serde_json::Map::new();
    output.insert("object".into(), Value::String("list".into()));
    output.insert("data".into(), Value::Array(data));
    Ok(Bytes::from(Value::Object(output).to_string()))
}
