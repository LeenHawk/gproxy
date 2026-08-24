use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Map, Value, json};

pub(super) fn request(body: &Bytes, model: &str, settings: &Value) -> Result<Bytes, ChannelError> {
    let mut input: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Bedrock video request JSON: {error}")))?;
    input.remove("model");
    let native_input = input.remove("model_input");
    let output = input.remove("outputDataConfig").or_else(|| {
        input
            .remove("output_s3_uri")
            .or_else(|| settings.get("video_output_s3_uri").cloned())
            .and_then(|value| value.as_str().map(str::to_owned))
            .map(|uri| json!({"s3OutputDataConfig":{"s3Uri":uri}}))
    });
    let model_input = native_input.unwrap_or_else(|| {
        let prompt = input
            .remove("prompt")
            .unwrap_or_else(|| Value::String(String::new()));
        let mut config = Map::new();
        if let Some(seconds) = input.remove("seconds") {
            let seconds = seconds
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .map(Value::from)
                .unwrap_or(seconds);
            config.insert("durationSeconds".into(), seconds);
        }
        if let Some(size) = input.remove("size") {
            config.insert("dimension".into(), size);
        }
        if let Some(seed) = input.remove("seed") {
            config.insert("seed".into(), seed);
        }
        config.entry("fps").or_insert_with(|| Value::from(24));
        config.extend(input);
        json!({
            "taskType":"TEXT_VIDEO",
            "textToVideoParams":{"text":prompt},
            "videoGenerationConfig":config
        })
    });
    let output = output.ok_or_else(|| {
        ChannelError::Prepare(
            "Bedrock video generation requires output_s3_uri or video_output_s3_uri".into(),
        )
    })?;
    encode(json!({
        "modelId":model,
        "modelInput":model_input,
        "outputDataConfig":output
    }))
}

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Bedrock video response JSON: {error}")))?;
    let arn = value
        .get("invocationArn")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let native = value
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let url = super::super::resource::output_url(&value);
    let root = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Observe("Bedrock video response is not an object".into()))?;
    if let Some(arn) = arn {
        root.insert(
            "id".into(),
            Value::String(super::super::resource::encode_arn(&arn)),
        );
    }
    if let Some(status) = native {
        root.insert(
            "status".into(),
            Value::String(
                match status.as_str() {
                    "Completed" => "completed",
                    "Failed" => "failed",
                    "InProgress" => "in_progress",
                    _ => "queued",
                }
                .into(),
            ),
        );
    }
    if let Some(url) = url {
        root.insert("url".into(), Value::String(url));
    }
    encode(value)
}

fn encode(value: Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
