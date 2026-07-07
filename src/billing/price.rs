//! §17 pricing: Decimal prices from structured price-rule fields.

use rust_decimal::Decimal;

use crate::store::persistence::records::PriceRule;
use crate::usage::NormalizedUsage;

/// Per-million-token token rates plus per-image item rate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pricing {
    pub input: Decimal,
    pub output: Decimal,
    pub cache_read: Decimal,
    pub cache_creation_5m: Decimal,
    pub cache_creation_1h: Decimal,
    /// Flat price PER IMAGE (not per-million) — image generation is billed by
    /// count, not tokens. Zero when unconfigured.
    pub image: Decimal,
}

/// Build [`Pricing`] from a structured price rule. Token prices are per
/// 1,000,000 tokens; image price is per generated image.
pub fn pricing_from_rule(rule: &PriceRule) -> Pricing {
    Pricing {
        input: rule.input_price,
        output: rule.output_price,
        cache_read: rule.cache_read_price,
        cache_creation_5m: rule.cache_creation_5m_price,
        cache_creation_1h: rule.cache_creation_1h_price,
        image: rule.image_price,
    }
}

/// Cost of `u` at rates `p`: Σ tokens × rate / 1_000_000 (exact Decimal math).
pub fn cost(u: &NormalizedUsage, p: &Pricing) -> Decimal {
    let million = Decimal::from(1_000_000u64);
    (Decimal::from(u.input) * p.input
        + Decimal::from(u.output) * p.output
        + Decimal::from(u.cache_read) * p.cache_read
        + Decimal::from(u.cache_creation_5m) * p.cache_creation_5m
        + Decimal::from(u.cache_creation_1h) * p.cache_creation_1h)
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
            cache_creation_1h_price: Decimal::from(6),
            image_price: "0.04".parse::<Decimal>().unwrap(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };
        let p = pricing_from_rule(&rule);
        assert_eq!(p.input, Decimal::from(3));
        assert_eq!(p.output, Decimal::from(15));
        assert_eq!(p.cache_read, "0.30".parse::<Decimal>().unwrap());
        assert_eq!(p.cache_creation_5m, "3.75".parse::<Decimal>().unwrap());
        assert_eq!(p.cache_creation_1h, Decimal::from(6));

        // Per-image flat rate (image generation is billed by count, not tokens).
        assert_eq!(p.image, "0.04".parse::<Decimal>().unwrap());
        assert_eq!(
            Decimal::from(3u64) * p.image,
            "0.12".parse::<Decimal>().unwrap()
        );

        // 1500 input @ 3.00/M = 0.0045; cache creation is split by TTL.
        let u = NormalizedUsage {
            input: 1500,
            output: 2000,
            cache_read: 10_000,
            cache_creation_5m: 200,
            cache_creation_1h: 300,
            reasoning: 0,
        };
        let expected: Decimal = "0.04005".parse().unwrap();
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
}
