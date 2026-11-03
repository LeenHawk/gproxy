use super::*;
use http::{HeaderMap, HeaderValue};
use protocol::{
    codec::{CodecLimits, MultipartEncoder},
    connection::MultipartPart,
    openai::audio as o,
};

pub(crate) fn file(bytes: Vec<u8>, mime: &str, filename: &str) -> Result<MultipartPart> {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_str(mime).map_err(Error::invalid)?,
    );
    headers.insert(
        http::header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "form-data; name=\"file\"; filename=\"{}\"",
            filename.replace('\\', "\\\\").replace('"', "\\\"")
        ))
        .map_err(Error::invalid)?,
    );
    Ok(MultipartPart {
        headers,
        body: HttpBody::Bytes(bytes.into()),
    })
}
fn add(parts: &mut Vec<MultipartPart>, name: &str, value: impl Serialize) -> Result<()> {
    let text = match serde_json::to_value(value).map_err(Error::invalid)? {
        serde_json::Value::String(s) => s,
        value => value.to_string(),
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "form-data; name=\"{}\"",
            name.replace('"', "\\\"")
        ))
        .map_err(Error::invalid)?,
    );
    parts.push(MultipartPart {
        headers,
        body: HttpBody::Bytes(text.into()),
    });
    Ok(())
}
pub(crate) fn transcription(
    form: o::CreateTranscriptionMultipartForm,
    rest: protocol::Rest,
    limits: CodecLimits,
) -> Result<WireRequest<HttpBody>> {
    let o::CreateTranscriptionMultipartForm {
        file,
        model,
        language,
        languages,
        keywords,
        prompt,
        response_format,
        temperature,
        timestamp_granularities,
        include,
        chunking_strategy,
        known_speaker_names,
        known_speaker_references,
        stream,
    } = form;
    let mut parts = vec![file];
    add(&mut parts, "model", model)?;
    if let Some(value) = language {
        add(&mut parts, "language", value)?;
    }
    if let Some(value) = prompt {
        add(&mut parts, "prompt", value)?;
    }
    if let Some(value) = response_format {
        add(&mut parts, "response_format", value)?;
    }
    if let Some(value) = temperature {
        add(&mut parts, "temperature", value)?;
    }
    if let Some(Some(value)) = stream {
        add(&mut parts, "stream", value)?;
    }
    if let Some(Some(value)) = chunking_strategy {
        add(&mut parts, "chunking_strategy", value)?;
    }
    for (name, values) in [
        ("languages[]", languages),
        ("keywords[]", keywords),
        ("known_speaker_names[]", known_speaker_names),
        ("known_speaker_references[]", known_speaker_references),
    ] {
        for value in values.into_iter().flatten() {
            add(&mut parts, name, value)?;
        }
    }
    for value in timestamp_granularities.into_iter().flatten() {
        add(&mut parts, "timestamp_granularities[]", value)?;
    }
    for value in include.into_iter().flatten() {
        add(&mut parts, "include[]", value)?;
    }
    for (name, value) in rest {
        add(&mut parts, &name, value)?;
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(Error::invalid)?;
    let boundary = format!("gproxy-{:032x}", u128::from_le_bytes(bytes));
    let encoder = MultipartEncoder::new(boundary.clone(), parts, limits).map_err(Error::invalid)?;
    let body = futures_util::stream::try_unfold(encoder, |mut encoder| async move {
        let chunk = encoder
            .next_chunk()
            .await
            .map_err(|e| Box::new(e) as protocol::connection::TransportError)?;
        Ok(chunk.map(|c| (c, encoder)))
    });
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
            .map_err(Error::invalid)?,
    );
    Ok(WireRequest {
        method: http::Method::POST,
        path: "/v1/audio/transcriptions".into(),
        query: None,
        headers,
        body: HttpBody::Stream(Box::pin(body)),
    })
}
