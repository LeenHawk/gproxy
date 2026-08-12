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
    pub tiers: Vec<PricingTier>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PricingTier {
    pub min_prompt_tokens: u64,
    pub input: Option<Decimal>,
    pub output: Option<Decimal>,
    pub cache_read: Option<Decimal>,
    pub cache_creation_5m: Option<Decimal>,
    pub cache_creation_30m: Option<Decimal>,
    pub cache_creation_1h: Option<Decimal>,
    pub image_output: Option<Decimal>,
}

/// Build [`Pricing`] from a structured price rule. All prices are per
/// 1,000,000 tokens.
pub fn pricing_from_rule(rule: &PriceRule) -> Pricing {
    Pricing {
        input: rule.input_price,
        output: rule.output_price,
        cache_read: rule.cache_read_price,
        cache_creation_5m: rule.cache_creation_5m_price,
        cache_creation_30m: rule.cache_creation_30m_price,
        cache_creation_1h: rule.cache_creation_1h_price,
        image_output: rule.image_output_price,
        tiers: parse_tiers(rule.pricing_tiers_json.as_ref()),
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
            let min_prompt_tokens = item.get("min_prompt_tokens")?.as_u64()?;
            Some(PricingTier {
                min_prompt_tokens,
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

/// Cost of `u` at rates `p`: Σ tokens × rate / 1_000_000 (exact Decimal math).
pub fn cost(u: &NormalizedUsage, p: &Pricing) -> Decimal {
    let million = Decimal::from(1_000_000u64);
    let prompt_tokens = u.input + u.cache_read + u.cache_creation();
    let tier = p
        .tiers
        .iter()
        .rev()
        .find(|tier| prompt_tokens >= tier.min_prompt_tokens);
    let rate = |base: Decimal, select: fn(&PricingTier) -> Option<Decimal>| {
        tier.and_then(select).unwrap_or(base)
    };
    (Decimal::from(u.input) * rate(p.input, |tier| tier.input)
        + Decimal::from(u.output) * rate(p.output, |tier| tier.output)
        + Decimal::from(u.cache_read) * rate(p.cache_read, |tier| tier.cache_read)
        + Decimal::from(u.cache_creation_5m)
            * rate(p.cache_creation_5m, |tier| tier.cache_creation_5m)
        + Decimal::from(u.cache_creation_30m)
            * rate(p.cache_creation_30m, |tier| tier.cache_creation_30m)
        + Decimal::from(u.cache_creation_1h)
            * rate(p.cache_creation_1h, |tier| tier.cache_creation_1h)
        + Decimal::from(u.image_output) * rate(p.image_output, |tier| tier.image_output))
        / million
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
}
