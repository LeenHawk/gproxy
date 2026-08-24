use gproxy_protocol::{aws, claude};
use serde_json::Value;

pub(super) fn usage(tokens: &aws::TokenUsage, tier: Option<&aws::ServiceTier>) -> claude::Usage {
    let mut cache_5m = 0_u64;
    let mut cache_1h = 0_u64;
    for detail in tokens.cache_details.iter().flatten() {
        match &detail.ttl {
            aws::CacheTtl::Known(aws::CacheTtlKnown::FiveMinutes) => {
                cache_5m = cache_5m.saturating_add(detail.input_tokens)
            }
            aws::CacheTtl::Known(aws::CacheTtlKnown::OneHour) => {
                cache_1h = cache_1h.saturating_add(detail.input_tokens)
            }
            aws::CacheTtl::Unknown(_) => {}
        }
    }
    let cache_creation = (cache_5m > 0 || cache_1h > 0).then(|| claude::CacheCreation {
        ephemeral_1h_input_tokens: cache_1h,
        ephemeral_5m_input_tokens: cache_5m,
        rest: Default::default(),
    });
    let (service_tier, speed) = tier.map_or((None, None), |tier| match &tier.type_ {
        aws::ServiceTierType::Known(aws::ServiceTierTypeKnown::Priority) => (
            Some(claude::UsageServiceTier::Known(
                claude::UsageServiceTierKnown::Priority,
            )),
            Some(claude::Speed::Known(claude::SpeedKnown::Fast)),
        ),
        aws::ServiceTierType::Known(aws::ServiceTierTypeKnown::Default) => (
            Some(claude::UsageServiceTier::Known(
                claude::UsageServiceTierKnown::Standard,
            )),
            None,
        ),
        other => (
            Some(claude::UsageServiceTier::Unknown(enum_string(other))),
            None,
        ),
    });
    let mut rest = tokens.rest.clone();
    rest.insert("total_tokens".into(), Value::from(tokens.total_tokens));
    claude::Usage {
        input_tokens: Some(tokens.input_tokens),
        output_tokens: Some(tokens.output_tokens),
        cache_creation_input_tokens: tokens.cache_write_input_tokens,
        cache_read_input_tokens: tokens.cache_read_input_tokens,
        cache_creation,
        output_tokens_details: None,
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier,
        speed,
        rest,
    }
}

pub(super) fn enum_string(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .expect("typed enum serializes")
        .as_str()
        .expect("string enum")
        .into()
}
