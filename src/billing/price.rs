//! §17 pricing: per-million-token Decimal rates from `provider_models.pricing_json`.

use rust_decimal::Decimal;
use serde_json::Value;

use crate::usage::NormalizedUsage;

/// Per-million-token rates. Missing/unconfigured keys are zero-priced —
/// tokens are still recorded, they just cost nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pricing {
    pub input: Decimal,
    pub output: Decimal,
    pub cache_read: Decimal,
    pub cache_creation: Decimal,
    /// Flat price PER IMAGE (not per-million) — image generation is billed by
    /// count, not tokens. Zero when unconfigured.
    pub image: Decimal,
}

/// Parse [`Pricing`] from a `pricing_json` value. Rates are Decimal-as-text
/// (the records money convention); plain JSON numbers are accepted too.
/// Missing or malformed keys fall back to 0.
pub fn pricing_from(pricing_json: Option<&Value>) -> Pricing {
    let Some(v) = pricing_json else {
        return Pricing::default();
    };
    Pricing {
        input: rate(v, "input"),
        output: rate(v, "output"),
        cache_read: rate(v, "cache_read"),
        cache_creation: rate(v, "cache_creation"),
        image: rate(v, "image"),
    }
}

fn rate(v: &Value, key: &str) -> Decimal {
    parse_rate(v.get(key)).unwrap_or(Decimal::ZERO)
}

/// Parse one rate value (string or number) to a Decimal; `None` for
/// absent/null/non-numeric (so callers can fall through tiers).
fn parse_rate(v: Option<&Value>) -> Option<Decimal> {
    match v? {
        Value::Null => None,
        Value::String(s) => s.parse::<Decimal>().ok(),
        Value::Number(n) => n.to_string().parse::<Decimal>().ok(),
        _ => None,
    }
}

/// Per-image price for an image-generation request. `pricing_json.image` is
/// either a flat scalar (one price per image) or an object keyed by tier —
/// looked up as `"{size}/{quality}"` → `"{size}"` → `"default"`. Unconfigured /
/// unmatched → 0 (recorded but free). Lets a model price 1024² vs 1792² (and
/// standard vs hd) differently instead of a single flat rate.
pub fn image_rate(
    pricing_json: Option<&Value>,
    size: Option<&str>,
    quality: Option<&str>,
) -> Decimal {
    let Some(image) = pricing_json.and_then(|v| v.get("image")) else {
        return Decimal::ZERO;
    };
    let Value::Object(tiers) = image else {
        // flat scalar
        return parse_rate(Some(image)).unwrap_or(Decimal::ZERO);
    };
    let candidates = [
        size.zip(quality).map(|(s, q)| format!("{s}/{q}")),
        size.map(str::to_owned),
        Some("default".to_owned()),
    ];
    candidates
        .into_iter()
        .flatten()
        .find_map(|k| parse_rate(tiers.get(&k)))
        .unwrap_or(Decimal::ZERO)
}

/// Cost of `u` at rates `p`: Σ tokens × rate / 1_000_000 (exact Decimal math).
pub fn cost(u: &NormalizedUsage, p: &Pricing) -> Decimal {
    let million = Decimal::from(1_000_000u64);
    (Decimal::from(u.input) * p.input
        + Decimal::from(u.output) * p.output
        + Decimal::from(u.cache_read) * p.cache_read
        + Decimal::from(u.cache_creation) * p.cache_creation)
        / million
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pricing_parse_and_exact_cost_math() {
        // Text rate, number rate, missing keys → 0.
        let p = pricing_from(Some(&json!({"input": "3.00", "output": 15})));
        assert_eq!(p.input, Decimal::from(3));
        assert_eq!(p.output, Decimal::from(15));
        assert_eq!(p.cache_read, Decimal::ZERO);
        assert_eq!(p.cache_creation, Decimal::ZERO);
        assert_eq!(pricing_from(None), Pricing::default());

        // Per-image flat rate (image generation is billed by count, not tokens).
        let img = pricing_from(Some(&json!({"image": "0.04"})));
        assert_eq!(img.image, "0.04".parse::<Decimal>().unwrap());
        assert_eq!(
            Decimal::from(3u64) * img.image,
            "0.12".parse::<Decimal>().unwrap()
        );

        // Tiered image pricing: "{size}/{quality}" → "{size}" → "default".
        let tiers = json!({"image": {
            "1024x1024": "0.04",
            "1792x1024/hd": "0.12",
            "default": "0.02"
        }});
        let d = |s: &str| s.parse::<Decimal>().unwrap();
        assert_eq!(
            image_rate(Some(&tiers), Some("1792x1024"), Some("hd")),
            d("0.12")
        );
        assert_eq!(
            image_rate(Some(&tiers), Some("1024x1024"), Some("hd")),
            d("0.04")
        ); // size fallback
        assert_eq!(image_rate(Some(&tiers), Some("999x999"), None), d("0.02")); // default
        assert_eq!(
            image_rate(Some(&json!({"image": "0.05"})), Some("1024x1024"), None),
            d("0.05")
        ); // flat
        assert_eq!(image_rate(None, Some("1024x1024"), None), Decimal::ZERO);

        // 1500 input @ 3.00/M = 0.0045; cache tokens are free here but counted.
        let u = NormalizedUsage {
            input: 1500,
            output: 2000,
            cache_read: 10_000,
            cache_creation: 500,
            reasoning: 0,
        };
        let expected: Decimal = "0.0345".parse().unwrap(); // 0.0045 + 2000*15/1e6
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
