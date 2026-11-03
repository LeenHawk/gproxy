use super::*;
pub(super) fn render_block(block: &Block, calls: &BTreeMap<&str, &str>) -> Result<Option<g::Part>> {
    if matches!(block.content, Content::Opaque) {
        return read_native(block, Vendor::Gemini);
    }
    let metadata = read_native::<PartMetadata>(block, Vendor::Gemini)?;
    if matches!(block.content, Content::Reasoning(_)) && metadata.is_none() {
        return Ok(None);
    }
    let had_metadata = metadata.is_some();
    let meta = metadata.unwrap_or_default();
    let mut part = native_part(meta.fields);
    match &block.content {
        Content::Text(text) => part.text = Some(text.text.clone()),
        Content::Reasoning(Reasoning { text }) => {
            part.text = Some(text.clone());
            part.thought = Some(true);
        }
        Content::Image(image) => {
            media::check_file_vendor(&image.source, Vendor::Gemini)?;
            let source = &image.source;
            render_asset(
                &mut part,
                source,
                image.media_type.as_deref(),
                None,
                meta.inline_data,
                meta.file_data,
            )?;
            set_detail(&mut part, &image.detail);
        }
        Content::Document(document) => {
            let (asset, mime) = media::document_bytes(document)?;
            media::check_file_vendor(&asset, Vendor::Gemini)?;
            let source = asset.as_ref();
            render_asset(
                &mut part,
                source,
                mime,
                document.title.as_deref(),
                meta.inline_data,
                meta.file_data,
            )?;
            set_detail(&mut part, &document.detail);
        }
        Content::File(file) => {
            media::check_file_vendor(&file.source, Vendor::Gemini)?;
            let source = &file.source;
            render_asset(
                &mut part,
                source,
                file.media_type.as_deref(),
                file.filename.as_deref(),
                meta.inline_data,
                meta.file_data,
            )?;
        }
        Content::Audio(audio) => {
            let mut mime = media::pcm_mime(&audio.format)?;
            if let Some(original) = meta
                .inline_data
                .as_ref()
                .and_then(|m| m.mime_type.as_deref())
                .or_else(|| meta.file_data.as_ref().and_then(|m| m.mime_type.as_deref()))
                && media::parse_audio_format(original)? == audio.format
            {
                mime = original.into();
            }
            media::check_file_vendor(&audio.source, Vendor::Gemini)?;
            let source = &audio.source;
            render_asset(
                &mut part,
                source,
                Some(&mime),
                None,
                meta.inline_data,
                meta.file_data,
            )?;
            if let Some(transcript) = &audio.transcript {
                let primary = meta.primary_transcript_chars;
                if primary != Some(0) {
                    let text = match primary {
                        Some(count) => transcript.chars().take(count).collect(),
                        None => transcript.clone(),
                    };
                    part.audio_transcription = Some(transcription(
                        text,
                        meta.audio_transcription.unwrap_or_default(),
                    ));
                }
            }
        }
        Content::Video(video) => {
            media::check_file_vendor(&video.source, Vendor::Gemini)?;
            let source = &video.source;
            render_asset(
                &mut part,
                source,
                video.media_type.as_deref(),
                None,
                meta.inline_data,
                meta.file_data,
            )?;
            if let Some(clip) = &video.clip {
                let value = part
                    .video_metadata
                    .get_or_insert_with(|| g::VideoMetadata::builder().build());
                value.start_offset = duration_field(value.start_offset.take(), clip.start_seconds)?;
                value.end_offset = duration_field(value.end_offset.take(), clip.end_seconds)?;
                value.fps = clip.fps;
            }
        }
        Content::ToolCall(call) => {
            if call.provider_executed {
                let Some(native) = meta.tool_call else {
                    return Ok(None);
                };
                let mut value = g::ToolCall::builder(native.tool_type)
                    .rest(native.rest)
                    .build();
                if native.id_present {
                    value.id = Some(call.id.clone());
                }
                if native.name_present {
                    value.tool_name = Some(call.name.clone());
                }
                if native.args_present || call.arguments != "{}" {
                    value.args =
                        Some(serde_json::from_str(&call.arguments).map_err(Error::invalid)?);
                }
                part.tool_call = Some(value);
                return Ok(Some(part));
            }
            let include_args = meta.function_call.as_ref().is_some_and(|m| m.args_present)
                || !had_metadata
                || call.arguments != "{}";
            let include_id =
                meta.function_call.as_ref().is_some_and(|m| m.id_present) || !had_metadata;
            let mut value = g::FunctionCall::builder(call.name.clone()).build();
            if let Some(native) = meta.function_call {
                value.rest = native.rest;
            }
            if include_args {
                value.args = Some(serde_json::from_str(&call.arguments).map_err(Error::invalid)?);
            }
            if include_id && !call.id.is_empty() {
                value.id = Some(call.id.clone());
            }
            part.function_call = Some(value);
        }
        Content::ToolResult(result) => {
            return render_tool_result(
                part,
                result,
                calls,
                meta.tool_response,
                meta.function_response,
                meta.response_json_text,
                had_metadata,
            );
        }
        Content::Unknown(_) => return Ok(None),
        Content::Opaque => return Err(Error::invalid("opaque Gemini part has no native value")),
    }
    Ok(Some(part))
}
