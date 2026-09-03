use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::Operation;
use gproxy_protocol::openai::common::ServiceTier;
use gproxy_protocol::openai::compact::CompactedResponseObject;
use gproxy_protocol::openai::generate_content::responses::{ResponseObject, ResponseUsage};
use gproxy_protocol::openai::images::{ImageUsage, ImagesResponse};
use gproxy_protocol::openai::search::SearchResponse;
use rust_decimal::Decimal;

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    match ctx.key.operation() {
        Operation::GenerateContent
        | Operation::StreamGenerateContent
        | Operation::GuardianReview
        | Operation::GuardianClassify => {
            let response: ResponseObject = serde_json::from_slice(ctx.response_body).ok()?;
            response
                .usage
                .as_ref()
                .map(|usage| from_response_with_tier(usage, response.service_tier.as_ref()))
        }
        Operation::CompactContent => {
            let response: CompactedResponseObject =
                serde_json::from_slice(ctx.response_body).ok()?;
            let mut usage = from_response(&response.usage);
            add_string_dimension(
                &mut usage,
                "service_tier",
                response.rest.get("service_tier"),
            );
            Some(usage)
        }
        Operation::CreateImage | Operation::EditImage => {
            let response: ImagesResponse = serde_json::from_slice(ctx.response_body).ok()?;
            let mut usage = from_image(response.usage.as_ref()?);
            if let Some(outputs) = response.data {
                usage
                    .metrics
                    .insert("image_outputs".into(), Decimal::from(outputs.len()));
            }
            if let Some(size) = response.size {
                usage.dimensions.insert("size".into(), size.as_str().into());
            }
            if let Some(quality) = response.quality {
                usage
                    .dimensions
                    .insert("quality".into(), quality.as_str().into());
            }
            Some(usage)
        }
        Operation::WebSearch => {
            let response: SearchResponse = serde_json::from_slice(ctx.response_body).ok()?;
            let mut usage =
                serde_json::from_value::<ResponseUsage>(response.rest.get("usage")?.clone())
                    .ok()
                    .map(|usage| from_response(&usage))?;
            add_string_dimension(
                &mut usage,
                "service_tier",
                response.rest.get("service_tier"),
            );
            Some(usage)
        }
        Operation::SummarizeMemory => None,
        _ => None,
    }
}

pub(super) fn from_response(usage: &ResponseUsage) -> NormalizedUsage {
    from_response_with_tier(usage, None)
}

pub(super) fn from_response_with_tier(
    usage: &ResponseUsage,
    tier: Option<&ServiceTier>,
) -> NormalizedUsage {
    let cached = usage
        .input_tokens_details
        .as_ref()
        .and_then(|details| details.cached_tokens);
    let write = usage
        .input_tokens_details
        .as_ref()
        .and_then(|details| details.cache_write_tokens)
        .filter(|tokens| *tokens > 0);
    let mut normalized = NormalizedUsage {
        input_tokens: u64::from(usage.input_tokens.saturating_sub(write.unwrap_or_default())),
        output_tokens: u64::from(usage.output_tokens),
        cached_input_tokens: cached.map(u64::from).unwrap_or_default(),
        ..Default::default()
    };
    if let Some(write) = write {
        normalized
            .metrics
            .insert("cache_creation_30m_tokens".into(), Decimal::from(write));
    }
    let reasoning = usage
        .output_tokens_details
        .as_ref()
        .and_then(|details| details.reasoning_tokens);
    if let Some(reasoning) = reasoning.filter(|tokens| *tokens > 0) {
        normalized
            .metrics
            .insert("reasoning_tokens".into(), Decimal::from(reasoning));
    }
    if let Some(searches) = usage
        .rest
        .get("server_tool_use_details")
        .or_else(|| usage.rest.get("server_tool_use"))
        .and_then(|details| details.get("web_search_requests"))
        .and_then(serde_json::Value::as_u64)
        .filter(|searches| *searches > 0)
    {
        normalized
            .metrics
            .insert("web_searches".into(), Decimal::from(searches));
    }
    if let Some(tier) = tier {
        normalized
            .dimensions
            .insert("service_tier".into(), tier.as_str().into());
    }
    normalized
}

fn from_image(usage: &ImageUsage) -> NormalizedUsage {
    let image_tokens = usage
        .output_tokens_details
        .as_ref()
        .map(|details| details.image_tokens)
        .unwrap_or(usage.output_tokens);
    let mut normalized = NormalizedUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens.saturating_sub(image_tokens),
        ..Default::default()
    };
    normalized
        .metrics
        .insert("image_output_tokens".into(), Decimal::from(image_tokens));
    normalized
}

fn add_string_dimension(
    usage: &mut NormalizedUsage,
    name: &str,
    value: Option<&serde_json::Value>,
) {
    if let Some(value) = value.and_then(serde_json::Value::as_str) {
        usage.dimensions.insert(name.into(), value.into());
    }
}
