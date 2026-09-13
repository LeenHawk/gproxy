use crate::{
    transform::{Report, TransformError},
    wire::{claude::content as c, gemini as g},
};
use serde_json::Value;
pub(super) fn direct_caller(caller: Option<c::ToolCaller>) -> Result<(), TransformError> {
    match caller {
        None | Some(c::Caller::Direct(_)) => Ok(()),
        Some(c::Caller::Server(_) | c::Caller::Server20260120(_)) => {
            Err(TransformError::unsupported(
                "tool_use.caller",
                "programmatic execution needs native host binding",
            ))
        }
    }
}
pub(super) fn to_gemini(
    block: c::ToolResultBlock,
    id: String,
    name: String,
    media: &super::media::MediaFacts,
    report: &mut Report,
) -> Result<g::FunctionResponse, TransformError> {
    let mut parts = Vec::new();
    let payload = match block.content {
        None => None,
        Some(c::ToolResultContent::Text(text)) => Some(Value::String(text)),
        Some(c::ToolResultContent::Blocks(blocks)) => {
            let mut texts = Vec::new();
            for block in blocks {
                match block {
                    c::ToolResultContentBlock::Text(v) => {
                        if v.citations.is_some() || v.cache_control.is_some() {
                            report.omitted(
                                "tool_result.text.metadata",
                                "Gemini has no citations/cache field",
                            );
                        }
                        texts.push(v.text);
                    }
                    c::ToolResultContentBlock::Image(v) => {
                        let p = super::media::image(v, media, report)?;
                        let blob = p.inline_data.ok_or_else(|| {
                            TransformError::missing_metadata("tool result image inline bytes")
                        })?;
                        parts.push(g::FunctionResponsePart::builder().inline_data(blob).build());
                    }
                    c::ToolResultContentBlock::Document(v) => {
                        for p in super::media::document(v, media, report)? {
                            if let Some(t) = p.text {
                                texts.push(t);
                            } else if let Some(b) = p.inline_data {
                                parts.push(
                                    g::FunctionResponsePart::builder().inline_data(b).build(),
                                );
                            } else {
                                return Err(TransformError::missing_metadata(
                                    "tool result document inline bytes",
                                ));
                            }
                        }
                    }
                    c::ToolResultContentBlock::SearchResult(_)
                    | c::ToolResultContentBlock::ToolReference(_) => {
                        return Err(TransformError::unsupported(
                            "tool_result",
                            "native result references need execution binding",
                        ));
                    }
                }
            }
            Some(Value::Array(texts.into_iter().map(Value::String).collect()))
        }
    };
    let mut response = crate::Rest::new();
    if let Some(payload) = payload {
        response.insert(
            if block.is_error == Some(true) {
                "error"
            } else {
                "output"
            }
            .into(),
            payload,
        );
    } else if block.is_error == Some(true) {
        response.insert("error".into(), Value::Null);
    }
    if block.cache_control.is_some() {
        report.omitted("tool_result.cache_control", "Gemini has no cache field");
    }
    let mut out = g::FunctionResponse::builder(name, response).id(id).build();
    if !parts.is_empty() {
        out.parts = Some(parts);
    }
    Ok(out)
}
pub(super) fn to_claude(
    mut source: g::FunctionResponse,
    id: String,
) -> Result<c::ToolResultBlock, TransformError> {
    if source.will_continue == Some(true)
        || source
            .scheduling
            .as_ref()
            .is_some_and(|s| !matches!(s, g::Scheduling::Unspecified))
    {
        return Err(TransformError::unsupported(
            "function_response.scheduling",
            "Claude lacks nonblocking result scheduling",
        ));
    }
    let is_error = source.response.contains_key("error");
    let text = if source.response.len() == 1 && (is_error || source.response.contains_key("output"))
    {
        let v = source
            .response
            .remove(if is_error { "error" } else { "output" })
            .expect("key checked");
        match v {
            Value::String(t) => t,
            v => v.to_string(),
        }
    } else {
        Value::Object(source.response).to_string()
    };
    let mut content = vec![c::ToolResultContentBlock::Text(
        c::TextBlock::builder(c::TextBlockType::Tag, text).build(),
    )];
    for p in source.parts.unwrap_or_default() {
        let block = super::media::inline(p.inline_data.ok_or_else(|| {
            TransformError::shape("function_response.parts", "inline_data required")
        })?)?;
        content.push(match block {
            c::ContentBlock::Image(v) => c::ToolResultContentBlock::Image(v),
            c::ContentBlock::Document(v) => c::ToolResultContentBlock::Document(v),
            _ => unreachable!("inline maps only media"),
        });
    }
    Ok(c::ToolResultBlock::builder(c::ToolResultBlockType::Tag, id)
        .content(c::ToolResultContent::Blocks(content))
        .is_error(is_error)
        .build())
}
