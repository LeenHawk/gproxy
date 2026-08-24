use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::Operation;
use gproxy_protocol::gemini;

pub(crate) fn rewrite(
    operation: Operation,
    body: &Bytes,
    upstream_model: &str,
) -> Result<Bytes, ChannelError> {
    if upstream_model.is_empty() {
        return Ok(body.clone());
    }
    let model = model_name(upstream_model);
    match operation {
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            let mut request: gemini::GenerateContentRequest =
                serde_json::from_slice(body).map_err(json_error)?;
            if request.model.is_some() {
                request.model = Some(model);
            }
            serde_json::to_vec(&request)
                .map(Bytes::from)
                .map_err(json_error)
        }
        Operation::CountTokens => {
            let mut request: gemini::CountTokensRequest =
                serde_json::from_slice(body).map_err(json_error)?;
            if request.model.is_some() {
                request.model = Some(model);
            }
            serde_json::to_vec(&request)
                .map(Bytes::from)
                .map_err(json_error)
        }
        Operation::CreateEmbedding => {
            let mut request: gemini::EmbedContentRequest =
                serde_json::from_slice(body).map_err(json_error)?;
            if request.model.is_some() {
                request.model = Some(model);
            }
            serde_json::to_vec(&request)
                .map(Bytes::from)
                .map_err(json_error)
        }
        Operation::BatchCreateEmbedding => {
            let mut request: gemini::BatchEmbedContentsRequest =
                serde_json::from_slice(body).map_err(json_error)?;
            for item in &mut request.requests {
                if item.model.is_some() {
                    item.model = Some(model.clone());
                }
            }
            serde_json::to_vec(&request)
                .map(Bytes::from)
                .map_err(json_error)
        }
        _ => Ok(body.clone()),
    }
}

fn model_name(model: &str) -> String {
    if model.starts_with("models/") {
        model.to_owned()
    } else {
        format!("models/{model}")
    }
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("Gemini request JSON: {error}"))
}
