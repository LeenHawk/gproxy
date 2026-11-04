use super::*;

fn media_url(part: &Value) -> Option<String> {
    part["uri"].as_str().map(str::to_owned).or_else(|| {
        Some(format!(
            "data:{};base64,{}",
            part["mime_type"].as_str()?,
            part["data"].as_str()?
        ))
    })
}

fn native_media(kind: &str, url: &str, mime: &str) -> Value {
    if let Some((header, data)) = url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
    {
        json!({"type":kind,"mime_type":header,"data":data})
    } else {
        json!({"type":kind,"mime_type":mime,"uri":url})
    }
}

pub(super) fn to_native(
    value: &Value,
    dialect: Dialect,
    report: &mut Report,
) -> Result<Vec<Value>, TransformError> {
    if let Some(text) = value.as_str() {
        return Ok(vec![json!({"type":"text","text":text})]);
    }
    let mut out = Vec::new();
    for part in array(value) {
        let kind = string(&part["type"]);
        let content = match (dialect, kind) {
            (_, "text" | "input_text" | "output_text") => {
                json!({"type":"text","text":part["text"]})
            }
            (Dialect::OpenAi | Dialect::OpenAiChat, "input_image" | "image_url") => {
                let url = part["image_url"].as_str().or_else(|| part["image_url"]["url"].as_str())
                    .ok_or_else(|| TransformError::unsupported("input.image", "file ids require an explicit media URL or inline data for Interactions"))?;
                native_media("image", url, "image/jpeg")
            }
            (Dialect::OpenAi, "input_file") => {
                if let Some(url) = part["file_url"]
                    .as_str()
                    .or_else(|| part["file_data"].as_str())
                {
                    native_media("document", url, "application/pdf")
                } else {
                    return Err(TransformError::unsupported(
                        "input.file_id",
                        "materialize the file as inline data or a URL before Interactions conversion",
                    ));
                }
            }
            (Dialect::OpenAiChat, "input_audio") => {
                let audio = &part["input_audio"];
                json!({"type":"audio","data":audio["data"],"mime_type":format!("audio/{}", string(&audio["format"]))})
            }
            (Dialect::Claude, "image" | "document") => {
                let source = &part["source"];
                match string(&source["type"]) {
                    "base64" => {
                        json!({"type":kind,"data":source["data"],"mime_type":source["media_type"]})
                    }
                    "url" => native_media(
                        kind,
                        string(&source["url"]),
                        if kind == "image" {
                            "image/jpeg"
                        } else {
                            "application/pdf"
                        },
                    ),
                    "text" => json!({"type":"text","text":source["data"]}),
                    _ => {
                        return Err(TransformError::unsupported(
                            "input.source",
                            "this file source has no native Interactions media representation",
                        ));
                    }
                }
            }
            _ => {
                report.omitted(
                    format!("content.{kind}"),
                    "content block has no target representation",
                );
                continue;
            }
        };
        out.push(content);
    }
    Ok(out)
}

pub(super) fn from_native(
    value: &Value,
    dialect: Dialect,
    report: &mut Report,
) -> Result<Vec<Value>, TransformError> {
    let mut out = Vec::new();
    let contents: Vec<&Value> = if value.is_object() {
        vec![value]
    } else {
        array(value).collect()
    };
    for part in contents {
        match string(&part["type"]) {
            "text" => out.push(json!({"type":if dialect == Dialect::OpenAi {"input_text"} else {"text"},"text":part["text"]})),
            "image" | "document" => {
                let kind = string(&part["type"]);
                let url = media_url(part).ok_or_else(|| TransformError::shape("input.media", "media requires uri or mime_type with data"))?;
                out.push(match dialect {
                    Dialect::OpenAi if kind == "image" => json!({"type":"input_image","image_url":url,"detail":"auto"}),
                    Dialect::OpenAi => json!({"type":"input_file", "file_url":url}),
                    Dialect::OpenAiChat if kind == "image" => json!({"type":"image_url","image_url":{"url":url}}),
                    Dialect::Claude => {
                        let source = if let Some(data) = part.get("data") {
                            json!({"type":"base64","data":data,"media_type":part["mime_type"]})
                        } else { json!({"type":"url","url":url}) };
                        json!({"type":kind,"source":source})
                    }
                    _ => { report.omitted("input.document", "Chat does not support native document content"); continue; }
                });
            }
            "audio" if dialect == Dialect::OpenAiChat && part["data"].is_string() => {
                let format = string(&part["mime_type"]).strip_prefix("audio/").unwrap_or("wav");
                out.push(json!({"type":"input_audio","input_audio":{"data":part["data"],"format":format}}));
            }
            kind => report.omitted(format!("input.{kind}"), "media modality has no equivalent in this target request"),
        }
    }
    Ok(out)
}
