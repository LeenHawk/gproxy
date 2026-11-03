use super::*;
use protocol::gemini as g;
pub(crate) fn wire(
    model: &str,
    body: &g::GenerateContentRequestBody,
    streaming: bool,
) -> Result<WireRequest<HttpBody>> {
    let mut wire = crate::client::wire_json(
        http::Method::POST,
        format!(
            "/v1beta/models/{}:{}",
            crate::client::path_segment(model),
            if streaming {
                "streamGenerateContent"
            } else {
                "generateContent"
            }
        ),
        body,
    )?;
    if streaming {
        wire.query = Some("alt=sse".into());
        wire.headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static("text/event-stream"),
        );
    }
    Ok(wire)
}
pub(crate) fn usage(value: Option<&g::UsageMetadata>) -> Result<OperationUsage> {
    let Some(u) = value else {
        return Ok(OperationUsage::default());
    };
    Ok(OperationUsage {
        tokens: Usage {
            input_tokens: optional_count(u.prompt_token_count)?,
            output_tokens: optional_count(u.candidates_token_count)?,
            reasoning_tokens: optional_count(u.thoughts_token_count)?,
            cache_read_tokens: optional_count(u.cached_content_token_count)?,
            ..Default::default()
        },
        total_tokens: optional_count(u.total_token_count)?,
        ..Default::default()
    })
}
pub(crate) fn response(value: &g::GenerateContentResponseBody) -> ResponseMetadata {
    ResponseMetadata {
        id: value.response_id.clone(),
        model: value.model_version.clone(),
        timestamp: None,
    }
}
pub(crate) fn finish(
    value: Option<g::FinishReason>,
    feedback: Option<&g::PromptFeedback>,
) -> Result<FinishReason> {
    let blocked = feedback
        .and_then(|f| f.block_reason)
        .is_some_and(|r| r != g::BlockReason::Unspecified);
    let raw = if let Some(value) = value {
        Some(
            serde_json::from_value::<String>(serde_json::to_value(value).map_err(Error::invalid)?)
                .map_err(Error::invalid)?,
        )
    } else {
        feedback
            .and_then(|f| f.block_reason)
            .filter(|r| *r != g::BlockReason::Unspecified)
            .map(|r| {
                serde_json::from_value::<String>(serde_json::to_value(r).map_err(Error::invalid)?)
                    .map_err(Error::invalid)
            })
            .transpose()?
    };
    use g::FinishReason as F;
    let unified = if blocked {
        FinishReasonKind::ContentBlocked
    } else {
        match value {
            Some(F::Stop) => FinishReasonKind::Stop,
            Some(F::MaxTokens) => FinishReasonKind::Length,
            Some(
                F::Safety
                | F::Recitation
                | F::Blocklist
                | F::ProhibitedContent
                | F::Spii
                | F::ImageSafety
                | F::ImageProhibitedContent
                | F::ImageRecitation,
            ) => FinishReasonKind::ContentBlocked,
            None | Some(F::Unspecified) => FinishReasonKind::Interrupted,
            _ => FinishReasonKind::Other,
        }
    };
    Ok(FinishReason::new(unified, raw))
}
#[cfg(any(feature = "speech", feature = "transcription"))]
pub(crate) fn blocked(value: &g::GenerateContentResponseBody) -> bool {
    value
        .prompt_feedback
        .as_ref()
        .and_then(|p| p.block_reason.as_ref())
        .is_some_and(|r| *r != g::BlockReason::Unspecified)
}
pub(crate) fn candidate_warning(
    value: &g::GenerateContentResponseBody,
    warnings: &mut Vec<Warning>,
) {
    if value.candidates.as_ref().is_some_and(|c| c.len() > 1) {
        compatibility(
            warnings,
            "response.candidates",
            "Only the first candidate is returned",
        );
    }
}

/// Provider-only observations; portable counts and response identity stay in IR.
pub(crate) fn extras(response: &g::GenerateContentResponseBody) -> Result<protocol::Rest> {
    #[derive(Serialize)]
    struct Details<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_tokens_details: &'a Option<Vec<g::ModalityTokenCount>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_tokens_details: &'a Option<Vec<g::ModalityTokenCount>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        candidates_tokens_details: &'a Option<Vec<g::ModalityTokenCount>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_prompt_tokens_details: &'a Option<Vec<g::ModalityTokenCount>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_prompt_token_count: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        service_tier: &'a Option<g::ServiceTier>,
        #[serde(flatten)]
        rest: &'a protocol::Rest,
    }
    #[derive(Serialize)]
    struct Extra<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_feedback: &'a Option<g::PromptFeedback>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage_details: Option<Details<'a>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_status: &'a Option<g::ModelStatus>,
        #[serde(flatten)]
        rest: &'a protocol::Rest,
    }
    let usage_details = response.usage_metadata.as_ref().map(|u| Details {
        prompt_tokens_details: &u.prompt_tokens_details,
        cache_tokens_details: &u.cache_tokens_details,
        candidates_tokens_details: &u.candidates_tokens_details,
        tool_use_prompt_tokens_details: &u.tool_use_prompt_tokens_details,
        tool_use_prompt_token_count: u.tool_use_prompt_token_count,
        service_tier: &u.service_tier,
        rest: &u.rest,
    });
    metadata(Extra {
        prompt_feedback: &response.prompt_feedback,
        usage_details,
        model_status: &response.model_status,
        rest: &response.rest,
    })
}

#[cfg(any(feature = "speech", feature = "transcription"))]
pub(crate) fn observe_extras(
    metadata: &mut protocol::Rest,
    response: &g::GenerateContentResponseBody,
) -> Result<()> {
    for (key, value) in extras(response)? {
        if key == "usage_details"
            && let Some(serde_json::Value::Object(previous)) = metadata.get_mut(&key)
            && let serde_json::Value::Object(next) = value
        {
            previous.extend(next);
        } else {
            metadata.insert(key, value);
        }
    }
    Ok(())
}
