//! §17 pricing: Decimal prices from structured price-rule fields.

use rust_decimal::Decimal;

use crate::store::persistence::records::PriceRule;
use crate::usage::NormalizedUsage;

/// Per-million-token rates for normalized usage categories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pricing {
    pub input: Decimal,
    pub output: Decimal,
    pub cache_read: Decimal,
    pub cache_creation_5m: Decimal,
    pub cache_creation_30m: Decimal,
    pub cache_creation_1h: Decimal,
    /// Per-million image-output-token price.
    pub image_output: Decimal,
    /// Normalized request service/speed tier used to select a modifier.
    pub service_tier: Option<String>,
    pub tiers: Vec<PricingTier>,
    pub metric_rates: Vec<crate::store::persistence::records::PriceRate>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PricingTier {
    /// Optional request service/speed tier. `None` keeps the existing
    /// prompt-length-only behavior.
    pub service_tier: Option<String>,
    /// Minimum total prompt tokens for this entry. Service-tier-only entries
    /// default to zero.
    pub min_prompt_tokens: u64,
    /// Multiplier applied after the matching prompt-length rates. Explicit
    /// category prices below take precedence over the multiplier.
    pub multiplier: Option<Decimal>,
    pub input: Option<Decimal>,
    pub output: Option<Decimal>,
    pub cache_read: Option<Decimal>,
    pub cache_creation_5m: Option<Decimal>,
    pub cache_creation_30m: Option<Decimal>,
    pub cache_creation_1h: Option<Decimal>,
    pub image_output: Option<Decimal>,
}

impl Pricing {
    pub fn with_service_tier(mut self, service_tier: Option<&str>) -> Self {
        self.service_tier = service_tier.and_then(normalize_service_tier);
        self
    }
}

/// Build [`Pricing`] from a structured price rule. All prices are per
/// 1,000,000 tokens.
pub fn pricing_from_rule(rule: &PriceRule) -> Pricing {
    let rates = rule.effective_rates();
    let rate = |metric: &str, fallback: Decimal| {
        rates
            .iter()
            .enumerate()
            .filter(|(_, rate)| rate.metric == metric && rate.conditions_json.is_none())
            .max_by_key(|(index, rate)| (rate.sort_order, *index))
            .map(|(_, rate)| rate)
            .map(|rate| {
                rate.price_usd * Decimal::from(1_000_000u64) / Decimal::from(rate.unit_size)
            })
            .unwrap_or(fallback)
    };
    Pricing {
        input: rate("input_tokens", rule.input_price),
        output: rate("output_tokens", rule.output_price),
        cache_read: rate("cache_read_tokens", rule.cache_read_price),
        cache_creation_5m: rate("cache_creation_5m_tokens", rule.cache_creation_5m_price),
        cache_creation_30m: rate("cache_creation_30m_tokens", rule.cache_creation_30m_price),
        cache_creation_1h: rate("cache_creation_1h_tokens", rule.cache_creation_1h_price),
        image_output: rate("image_output_tokens", rule.image_output_price),
        service_tier: None,
        tiers: parse_tiers(rule.pricing_tiers_json.as_ref()),
        metric_rates: rates,
    }
}

fn parse_tiers(value: Option<&serde_json::Value>) -> Vec<PricingTier> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let decimal = |item: &serde_json::Value, key: &str| {
        item.get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.parse().ok())
    };
    let mut tiers: Vec<_> = items
        .iter()
        .filter_map(|item| {
            let service_tier = item
                .get("service_tier")
                .and_then(serde_json::Value::as_str)
                .and_then(normalize_service_tier);
            let has_prompt_threshold = item.get("min_prompt_tokens").is_some();
            let min_prompt_tokens = match item.get("min_prompt_tokens") {
                Some(value) => value.as_u64()?,
                None => 0,
            };
            // Ignore objects that select neither a prompt threshold nor a
            // service tier. This preserves the old malformed-entry behavior.
            if service_tier.is_none() && !has_prompt_threshold {
                return None;
            }
            Some(PricingTier {
                service_tier,
                min_prompt_tokens,
                multiplier: decimal(item, "multiplier").filter(|value| *value >= Decimal::ZERO),
                input: decimal(item, "input_price"),
                output: decimal(item, "output_price"),
                cache_read: decimal(item, "cache_read_price"),
                cache_creation_5m: decimal(item, "cache_creation_5m_price"),
                cache_creation_30m: decimal(item, "cache_creation_30m_price"),
                cache_creation_1h: decimal(item, "cache_creation_1h_price"),
                image_output: decimal(item, "image_output_price"),
            })
        })
        .collect();
    tiers.sort_by_key(|tier| tier.min_prompt_tokens);
    tiers
}

/// Normalize a provider-defined service tier for matching. Tier names stay
/// otherwise opaque, so custom providers can use values unknown to gproxy.
pub fn normalize_service_tier(value: &str) -> Option<String> {
    let mut normalized = value.trim().to_ascii_lowercase().replace('-', "_");
    normalized = match normalized.as_str() {
        // OpenAI and OpenRouter document `fast` as an alias of `priority`;
        // Claude reports the same accelerated class as `usage.speed = fast`.
        "fast" => "priority".into(),
        "ultra_fast" => "ultrafast".into(),
        // Providers use different names for their ordinary pay-as-you-go tier.
        "default" | "on_demand" => "standard".into(),
        _ => normalized,
    };
    (!normalized.is_empty()).then_some(normalized)
}

fn tier_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("type").and_then(serde_json::Value::as_str))
        .and_then(normalize_service_tier)
}

/// Read the actual tier reported by a provider response. Supported shapes
/// include OpenAI/OpenRouter/xAI top-level `service_tier`, Claude/Bedrock
/// `usage.speed` / `usage.service_tier`, Gemini `usageMetadata.serviceTier`,
/// and the nested objects used by Responses and Claude stream events.
pub fn response_service_tier_from_value(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    ["speed", "service_tier", "serviceTier"]
        .into_iter()
        .find_map(|key| object.get(key).and_then(tier_value))
        .or_else(|| {
            ["usage", "usageMetadata", "response", "message"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(response_service_tier_from_value))
        })
}

pub fn response_service_tier(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    response_service_tier_from_value(&value)
}

/// Gemini reports graceful Priority-to-Standard downgrades in this response
/// header rather than the JSON body.
pub fn response_service_tier_from_headers(headers: &http::HeaderMap) -> Option<String> {
    headers
        .get("x-gemini-service-tier")
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_service_tier)
}

/// Override request-derived pricing only when the upstream reported an actual
/// serving tier. An absent report intentionally keeps the request tier as a
/// best-effort fallback.
pub fn apply_actual_service_tier(pricing: &mut Pricing, actual: Option<&str>) {
    if let Some(actual) = actual.and_then(normalize_service_tier) {
        pricing.service_tier = Some(actual);
    }
}

/// Read the requested service/speed tier from any supported JSON wire shape.
/// `speed` wins because Claude may carry both `service_tier: auto` and the
/// independently billable `speed: fast` field.
pub fn request_service_tier(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let object = value.as_object()?;
    ["speed", "service_tier", "serviceTier"]
        .into_iter()
        .find_map(|key| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(normalize_service_tier)
        })
}

/// Cost of `u` at rates `p`: Σ tokens × rate / 1_000_000 (exact Decimal math).
pub fn cost(u: &NormalizedUsage, p: &Pricing) -> Decimal {
    let million = Decimal::from(1_000_000u64);
    let prompt_tokens = u.input + u.cache_read + u.cache_creation();
    let prompt_tier = p
        .tiers
        .iter()
        .rev()
        .find(|tier| tier.service_tier.is_none() && prompt_tokens >= tier.min_prompt_tokens);
    let service_tier = p.service_tier.as_deref().and_then(|requested| {
        p.tiers.iter().rev().find(|tier| {
            tier.service_tier.as_deref() == Some(requested)
                && prompt_tokens >= tier.min_prompt_tokens
        })
    });
    let rate = |base: Decimal, select: fn(&PricingTier) -> Option<Decimal>| {
        let prompt_rate = prompt_tier.and_then(select).unwrap_or(base);
        service_tier.and_then(select).unwrap_or_else(|| {
            prompt_rate
                * service_tier
                    .and_then(|tier| tier.multiplier)
                    .unwrap_or(Decimal::ONE)
        })
    };
    let metric_price = |metric: &str, fallback: Decimal| {
        selected_metric_rate(metric, u, p)
            .map(|selected| selected.price_usd * million / Decimal::from(selected.unit_size.max(1)))
            .unwrap_or(fallback)
    };
    let token_cost = (Decimal::from(u.input)
        * rate(metric_price("input_tokens", p.input), |tier| tier.input)
        + Decimal::from(u.output)
            * rate(metric_price("output_tokens", p.output), |tier| tier.output)
        + Decimal::from(u.cache_read)
            * rate(metric_price("cache_read_tokens", p.cache_read), |tier| {
                tier.cache_read
            })
        + Decimal::from(u.cache_creation_5m)
            * rate(
                metric_price("cache_creation_5m_tokens", p.cache_creation_5m),
                |tier| tier.cache_creation_5m,
            )
        + Decimal::from(u.cache_creation_30m)
            * rate(
                metric_price("cache_creation_30m_tokens", p.cache_creation_30m),
                |tier| tier.cache_creation_30m,
            )
        + Decimal::from(u.cache_creation_1h)
            * rate(
                metric_price("cache_creation_1h_tokens", p.cache_creation_1h),
                |tier| tier.cache_creation_1h,
            )
        + Decimal::from(u.image_output)
            * rate(
                metric_price("image_output_tokens", p.image_output),
                |tier| tier.image_output,
            ))
        / million;
    let mut dimensional = Decimal::ZERO;
    let mut minimum = Decimal::ZERO;
    let mut metrics: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for item in &p.metric_rates {
        if token_metric(&item.metric) || !metrics.insert(&item.metric) {
            continue;
        }
        let Some(item) = selected_metric_rate(&item.metric, u, p) else {
            continue;
        };
        if item.metric == "minimum_cost" {
            minimum = minimum.max(item.price_usd);
            continue;
        }
        let quantity = if item.metric == "request" {
            Decimal::ONE
        } else {
            u.metric(&item.metric)
        };
        if quantity > Decimal::ZERO && item.unit_size > 0 {
            dimensional += quantity * item.price_usd / Decimal::from(item.unit_size);
        }
    }
    (token_cost + dimensional).max(minimum)
}

fn selected_metric_rate<'a>(
    metric: &str,
    usage: &NormalizedUsage,
    pricing: &'a Pricing,
) -> Option<&'a crate::store::persistence::records::PriceRate> {
    pricing
        .metric_rates
        .iter()
        .enumerate()
        .filter(|(_, rate)| rate.metric == metric && rate_conditions_match(rate, usage, pricing))
        .max_by_key(|(index, rate)| (rate.sort_order, *index))
        .map(|(_, rate)| rate)
}

fn token_metric(metric: &str) -> bool {
    matches!(
        metric,
        "input_tokens"
            | "output_tokens"
            | "cache_read_tokens"
            | "cache_creation_5m_tokens"
            | "cache_creation_30m_tokens"
            | "cache_creation_1h_tokens"
            | "image_output_tokens"
    )
}

fn rate_conditions_match(
    rate: &crate::store::persistence::records::PriceRate,
    usage: &NormalizedUsage,
    pricing: &Pricing,
) -> bool {
    let Some(conditions) = rate
        .conditions_json
        .as_ref()
        .and_then(serde_json::Value::as_object)
    else {
        return true;
    };
    for (key, expected) in conditions {
        if key == "service_tier" {
            if expected.as_str() != pricing.service_tier.as_deref() {
                return false;
            }
        } else if key == "min_prompt_tokens" {
            if expected.as_u64().is_some_and(|minimum| {
                usage.input + usage.cache_read + usage.cache_creation() <= minimum
            }) {
                return false;
            }
        } else if key == "utc_start" || key == "utc_end" {
            let start = conditions
                .get("utc_start")
                .and_then(serde_json::Value::as_u64);
            let end = conditions
                .get("utc_end")
                .and_then(serde_json::Value::as_u64);
            if key == "utc_start" && !utc_window_matches(start, end) {
                return false;
            }
        } else {
            let expected = match expected {
                serde_json::Value::String(value) => Some(value.clone()),
                serde_json::Value::Bool(value) => Some(value.to_string()),
                serde_json::Value::Number(value) => Some(value.to_string()),
                _ => None,
            };
            if let Some(expected) = expected
                && usage.dimensions.get(key) != Some(&expected)
            {
                return false;
            }
        }
    }
    true
}

fn utc_window_matches(start: Option<u64>, end: Option<u64>) -> bool {
    let (Some(start), Some(end)) = (start, end) else {
        return true;
    };
    let clock_minutes = |hhmm: u64| (hhmm / 100) * 60 + hhmm % 100;
    let start = clock_minutes(start);
    let end = clock_minutes(end);
    let now = crate::util::time::unix_now().rem_euclid(86_400) as u64 / 60;
    if start <= end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_parse_and_exact_cost_math() {
        let rule = PriceRule {
            id: 1,
            provider_id: Some(1),
            match_type: "exact".into(),
            model_match: "gpt-test".into(),
            input_price: Decimal::from(3),
            output_price: Decimal::from(15),
            cache_read_price: "0.30".parse::<Decimal>().unwrap(),
            cache_creation_5m_price: "3.75".parse::<Decimal>().unwrap(),
            cache_creation_30m_price: "3.75".parse::<Decimal>().unwrap(),
            cache_creation_1h_price: Decimal::from(6),
            image_output_price: Decimal::from(40),
            pricing_tiers_json: None,
            rates: Vec::new(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };
        let p = pricing_from_rule(&rule);
        assert_eq!(p.input, Decimal::from(3));
        assert_eq!(p.output, Decimal::from(15));
        assert_eq!(p.cache_read, "0.30".parse::<Decimal>().unwrap());
        assert_eq!(p.cache_creation_5m, "3.75".parse::<Decimal>().unwrap());
        assert_eq!(p.cache_creation_30m, "3.75".parse::<Decimal>().unwrap());
        assert_eq!(p.cache_creation_1h, Decimal::from(6));
        assert_eq!(p.image_output, Decimal::from(40));

        // 1500 input @ 3.00/M = 0.0045; cache creation is split by TTL.
        let u = NormalizedUsage {
            input: 1500,
            output: 2000,
            cache_read: 10_000,
            cache_creation_5m: 200,
            cache_creation_30m: 400,
            cache_creation_1h: 300,
            image_output: 100,
            reasoning: 0,
            ..Default::default()
        };
        let expected: Decimal = "0.04555".parse().unwrap();
        assert_eq!(cost(&u, &p), expected);
        assert_eq!(
            cost(
                &NormalizedUsage {
                    input: 1500,
                    ..Default::default()
                },
                &p
            ),
            "0.0045".parse().unwrap()
        );
    }

    #[test]
    fn tiered_pricing_uses_total_prompt_tokens_and_per_category_rates() {
        let pricing = Pricing {
            input: Decimal::from(2),
            output: Decimal::from(6),
            cache_read: "0.5".parse().unwrap(),
            tiers: vec![PricingTier {
                min_prompt_tokens: 200_000,
                input: Some(Decimal::from(4)),
                output: Some(Decimal::from(9)),
                cache_read: Some(Decimal::from(1)),
                ..Default::default()
            }],
            ..Default::default()
        };
        let short = NormalizedUsage {
            input: 189_999,
            cache_read: 10_000,
            output: 1_000,
            ..Default::default()
        };
        let long = NormalizedUsage {
            input: 190_000,
            cache_read: 10_000,
            output: 1_000,
            ..Default::default()
        };
        assert_eq!(cost(&short, &pricing), "0.390998".parse().unwrap());
        assert_eq!(cost(&long, &pricing), "0.779".parse().unwrap());
    }

    #[test]
    fn service_tier_multiplier_composes_with_prompt_tier_and_explicit_rates() {
        let pricing = Pricing {
            input: Decimal::from(2),
            output: Decimal::from(6),
            service_tier: Some("ultrafast".into()),
            tiers: vec![
                PricingTier {
                    min_prompt_tokens: 200_000,
                    input: Some(Decimal::from(4)),
                    output: Some(Decimal::from(9)),
                    ..Default::default()
                },
                PricingTier {
                    service_tier: Some("ultrafast".into()),
                    min_prompt_tokens: 0,
                    multiplier: Some(Decimal::from(3)),
                    output: Some(Decimal::from(30)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let usage = NormalizedUsage {
            input: 200_000,
            output: 1_000_000,
            ..Default::default()
        };

        // Long-context input is $4/M, then ×3; explicit ultrafast output
        // overrides both the base $6/M and long-context $9/M rates.
        assert_eq!(cost(&usage, &pricing), "32.4".parse().unwrap());
    }

    #[test]
    fn parses_and_normalizes_service_tier_entries() {
        let rule = PriceRule {
            id: 1,
            provider_id: None,
            match_type: "contains".into(),
            model_match: "model".into(),
            input_price: Decimal::ONE,
            output_price: Decimal::ONE,
            cache_read_price: Decimal::ZERO,
            cache_creation_5m_price: Decimal::ZERO,
            cache_creation_30m_price: Decimal::ZERO,
            cache_creation_1h_price: Decimal::ZERO,
            image_output_price: Decimal::ZERO,
            pricing_tiers_json: Some(serde_json::json!([
                {"service_tier": "Ultra-Fast", "multiplier": "4"}
            ])),
            rates: Vec::new(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };
        let pricing = pricing_from_rule(&rule);
        assert_eq!(pricing.tiers.len(), 1);
        assert_eq!(pricing.tiers[0].service_tier.as_deref(), Some("ultrafast"));
        assert_eq!(pricing.tiers[0].min_prompt_tokens, 0);
        assert_eq!(pricing.tiers[0].multiplier, Some(Decimal::from(4)));
    }

    #[test]
    fn extracts_service_tier_across_wire_shapes() {
        assert_eq!(
            request_service_tier(br#"{"service_tier":"Ultra-Fast"}"#).as_deref(),
            Some("ultrafast")
        );
        assert_eq!(
            request_service_tier(br#"{"serviceTier":"PRIORITY"}"#).as_deref(),
            Some("priority")
        );
        assert_eq!(
            request_service_tier(br#"{"service_tier":"auto","speed":"fast"}"#).as_deref(),
            Some("priority")
        );
        assert_eq!(request_service_tier(b"not json"), None);
    }

    #[test]
    fn independent_metric_rates_add_media_and_request_costs() {
        use crate::store::persistence::records::PriceRate;
        let pricing = Pricing {
            metric_rates: vec![
                PriceRate {
                    metric: "audio_seconds".into(),
                    unit: "second".into(),
                    unit_size: 1,
                    price_usd: "0.005".parse().unwrap(),
                    conditions_json: None,
                    sort_order: 0,
                },
                PriceRate {
                    metric: "audio_seconds".into(),
                    unit: "second".into(),
                    unit_size: 1,
                    price_usd: "0.01".parse().unwrap(),
                    conditions_json: Some(serde_json::json!({"quality": "hd"})),
                    sort_order: 1,
                },
                PriceRate {
                    metric: "request".into(),
                    unit: "request".into(),
                    unit_size: 1,
                    price_usd: "0.002".parse().unwrap(),
                    conditions_json: None,
                    sort_order: 2,
                },
            ],
            ..Default::default()
        };
        let mut usage = NormalizedUsage::default();
        usage.set_metric("audio_seconds", Decimal::from(12));
        usage.dimensions.insert("quality".into(), "hd".into());
        assert_eq!(cost(&usage, &pricing), "0.122".parse().unwrap());
    }

    #[test]
    fn normalizes_provider_aliases_and_extracts_actual_response_tiers() {
        assert_eq!(normalize_service_tier("fast").as_deref(), Some("priority"));
        assert_eq!(
            normalize_service_tier("on-demand").as_deref(),
            Some("standard")
        );
        assert_eq!(
            response_service_tier(br#"{"service_tier":"default"}"#).as_deref(),
            Some("standard")
        );
        assert_eq!(
            response_service_tier(br#"{"usage":{"speed":"fast"}}"#).as_deref(),
            Some("priority")
        );
        assert_eq!(
            response_service_tier(br#"{"usageMetadata":{"serviceTier":"FLEX"}}"#).as_deref(),
            Some("flex")
        );
        assert_eq!(
            response_service_tier(
                br#"{"type":"response.completed","response":{"service_tier":"priority"}}"#
            )
            .as_deref(),
            Some("priority")
        );
        assert_eq!(
            response_service_tier(
                br#"{"type":"message_start","message":{"usage":{"speed":"standard"}}}"#
            )
            .as_deref(),
            Some("standard")
        );
    }
}
