use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::Operation;
use gproxy_protocol::aws::{
    CacheTtl, CacheTtlKnown, ConverseResponse, CountTokensResponse, ServiceTier, ServiceTierType,
    ServiceTierTypeKnown, TokenUsage,
};
use rust_decimal::Decimal;

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    match ctx.key.operation {
        Operation::CountTokens => {
            let response: CountTokensResponse = serde_json::from_slice(ctx.response_body).ok()?;
            Some(NormalizedUsage {
                input_tokens: response.input_tokens,
                ..Default::default()
            })
        }
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            serde_json::from_slice::<ConverseResponse>(ctx.response_body)
                .ok()
                .map(|response| from_tokens(&response.usage, response.service_tier.as_ref()))
                .or_else(|| crate::shared::claude::usage::from_body(ctx.response_body))
        }
        Operation::RetrieveVideo => {
            let value: serde_json::Value = serde_json::from_slice(ctx.response_body).ok()?;
            (value.get("status").and_then(serde_json::Value::as_str) == Some("Completed")).then(
                || {
                    let mut usage = NormalizedUsage::default();
                    usage.metrics.insert("video_outputs".into(), Decimal::ONE);
                    usage
                },
            )
        }
        _ => None,
    }
}

pub(super) fn from_tokens(
    tokens: &TokenUsage,
    service_tier: Option<&ServiceTier>,
) -> NormalizedUsage {
    let cached = tokens.cache_read_input_tokens.unwrap_or_default();
    let mut usage = NormalizedUsage {
        input_tokens: tokens.input_tokens.saturating_add(cached),
        output_tokens: tokens.output_tokens,
        cached_input_tokens: cached,
        ..Default::default()
    };
    let mut cache_5m = 0_u64;
    let mut cache_1h = 0_u64;
    for detail in tokens.cache_details.iter().flatten() {
        match &detail.ttl {
            CacheTtl::Known(CacheTtlKnown::FiveMinutes) => {
                cache_5m = cache_5m.saturating_add(detail.input_tokens);
            }
            CacheTtl::Known(CacheTtlKnown::OneHour) => {
                cache_1h = cache_1h.saturating_add(detail.input_tokens);
            }
            CacheTtl::Unknown(_) => {}
        }
    }
    if cache_5m == 0 && cache_1h == 0 {
        cache_5m = tokens.cache_write_input_tokens.unwrap_or_default();
    }
    add(&mut usage, "cache_creation_5m_tokens", cache_5m);
    add(&mut usage, "cache_creation_1h_tokens", cache_1h);
    if let Some(tier) = service_tier {
        let (name, fast) = match &tier.type_ {
            ServiceTierType::Known(ServiceTierTypeKnown::Priority) => ("priority", true),
            ServiceTierType::Known(ServiceTierTypeKnown::Default) => ("standard", false),
            ServiceTierType::Known(ServiceTierTypeKnown::Flex) => ("flex", false),
            ServiceTierType::Known(ServiceTierTypeKnown::Reserved) => ("reserved", false),
            ServiceTierType::Unknown(name) => (name.as_str(), false),
        };
        usage.dimensions.insert("service_tier".into(), name.into());
        if fast {
            usage.dimensions.insert("speed".into(), "fast".into());
        }
    }
    usage
}

fn add(usage: &mut NormalizedUsage, name: &str, value: u64) {
    if value > 0 {
        usage.metrics.insert(name.into(), Decimal::from(value));
    }
}
