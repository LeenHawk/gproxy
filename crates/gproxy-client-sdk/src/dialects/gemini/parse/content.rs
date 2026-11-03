use super::*;
pub(super) fn parse_function_call(
    call: g::FunctionCall,
    index: u32,
    meta: &mut PartMetadata,
) -> Result<Content> {
    Ok({
        let g::FunctionCall {
            name,
            id,
            args,
            rest,
        } = call;
        let portable_id = id.clone().unwrap_or_else(|| format!("call_gemini_{index}"));
        let arguments = match &args {
            Some(args) => serde_json::to_string(args).map_err(Error::invalid)?,
            None => "{}".into(),
        };
        meta.function_call = Some(FunctionMetadata {
            id_present: id.is_some(),
            args_present: args.is_some(),
            rest,
        });
        Content::ToolCall(ToolCall {
            id: portable_id,
            name,
            arguments,
            provider_executed: false,
        })
    })
}
pub(super) fn parse_function_response(
    output: g::FunctionResponse,
    meta: &mut PartMetadata,
) -> Result<Content> {
    Ok({
        let g::FunctionResponse {
            name,
            response,
            id,
            parts,
            will_continue,
            scheduling,
            rest,
        } = output;
        let portable_id = id.clone().unwrap_or_default();
        let is_error = response.contains_key("error");
        let parts_present = parts.is_some();
        let output = if let Some(parts) = parts.filter(|p| !p.is_empty()) {
            let mut blocks = vec![ContentBlock::new(ToolResultContent::Text(Text {
                refusal: false,
                text: serde_json::to_string(&response).map_err(Error::invalid)?,
                citations: Vec::new(),
            }))];
            for part in parts {
                blocks.push(parse_response_part(part)?);
            }
            meta.response_json_text = true;
            ToolOutput::Blocks(blocks)
        } else {
            ToolOutput::Json(Value::Object(response))
        };
        meta.function_response = Some(ResultMetadata {
            id_present: id.is_some(),
            parts_present,
            will_continue,
            scheduling,
            rest,
        });
        Content::ToolResult(ToolResult {
            id: portable_id,
            name,
            output,
            is_error,
            provider_executed: false,
        })
    })
}
pub(super) fn parse_inline(
    blob: g::Blob,
    detail: Detail,
    fields: &mut g::Part,
    meta: &mut PartMetadata,
) -> Result<Content> {
    Ok({
        let (mut content, inline, audio) =
            parse_blob(blob, detail, fields.audio_transcription.take())?;
        meta.inline_data = Some(inline);
        meta.audio_transcription = audio;
        if let Content::Video(video) = &mut content {
            video.clip = parse_video(fields.video_metadata.as_ref())?;
        }
        content
    })
}
pub(super) fn parse_file_part(
    file: g::FileData,
    detail: Detail,
    fields: &mut g::Part,
    meta: &mut PartMetadata,
) -> Result<Content> {
    Ok({
        let (content, mut file) = parse_file(file, detail)?;
        let mut content = if fields.video_metadata.is_some() {
            match content {
                Content::File(value) => {
                    file.display_name = value.filename;
                    Content::Video(Video {
                        source: value.source,
                        media_type: value.media_type,
                        clip: None,
                    })
                }
                other => other,
            }
        } else {
            content
        };
        meta.file_data = Some(file);
        if let Content::Audio(audio) = &mut content
            && let Some(mut value) = fields.audio_transcription.take()
        {
            audio.transcript = Some(std::mem::take(&mut value.text));
            meta.audio_transcription = Some(audio_metadata(value));
        }
        if let Content::Video(video) = &mut content {
            video.clip = parse_video(fields.video_metadata.as_ref())?;
        }
        content
    })
}
pub(super) fn parse_server_call(
    call: g::ToolCall,
    index: u32,
    meta: &mut PartMetadata,
) -> Result<Content> {
    Ok({
        let g::ToolCall {
            tool_type,
            id,
            tool_name,
            args,
            rest,
        } = call;
        let name = match &tool_name {
            Some(name) => name.clone(),
            None => {
                serde_json::from_value(serde_json::to_value(&tool_type).map_err(Error::invalid)?)
                    .map_err(Error::invalid)?
            }
        };
        let arguments = match &args {
            Some(args) => serde_json::to_string(args).map_err(Error::invalid)?,
            None => "{}".into(),
        };
        let portable_id = id
            .clone()
            .unwrap_or_else(|| format!("server_gemini_{index}"));
        meta.tool_call = Some(ServerCallMetadata {
            tool_type,
            id_present: id.is_some(),
            name_present: tool_name.is_some(),
            args_present: args.is_some(),
            rest,
        });
        Content::ToolCall(ToolCall {
            id: portable_id,
            name,
            arguments,
            provider_executed: true,
        })
    })
}
pub(super) fn parse_server_result(
    result: g::ToolResponse,
    meta: &mut PartMetadata,
) -> Result<Content> {
    Ok({
        let g::ToolResponse {
            tool_type,
            id,
            response,
            rest,
        } = result;
        let name =
            serde_json::from_value(serde_json::to_value(&tool_type).map_err(Error::invalid)?)
                .map_err(Error::invalid)?;
        let output = ToolOutput::Json(
            response
                .as_ref()
                .map(|r| Value::Object(r.clone()))
                .unwrap_or(Value::Null),
        );
        let portable_id = id.clone().unwrap_or_default();
        meta.tool_response = Some(ServerResponseMetadata {
            tool_type,
            id_present: id.is_some(),
            response_present: response.is_some(),
            rest,
        });
        Content::ToolResult(ToolResult {
            id: portable_id,
            name,
            output,
            is_error: false,
            provider_executed: true,
        })
    })
}
