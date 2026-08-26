use std::collections::BTreeMap;

use http::{HeaderMap, HeaderValue};
use rust_decimal::Decimal;

use crate::control::{Pricing, PricingTier, response_service_tier};
use crate::usage::NormalizedUsage;

#[test]
fn prompt_axis_selects_the_highest_absolute_step() {
    let pricing = Pricing {
        input_per_million: Decimal::ONE,
        output_per_million: Decimal::from(2),
        cached_input_per_million: None,
        service_tier: None,
        tiers: vec![
            PricingTier {
                min_prompt_tokens: 500_000,
                input_per_million: Some(Decimal::from(3)),
                ..Default::default()
            },
            PricingTier {
                min_prompt_tokens: 100,
                input_per_million: Some(Decimal::from(2)),
                ..Default::default()
            },
        ],
        metric_rates: BTreeMap::new(),
    };
    let usage = NormalizedUsage {
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        ..Default::default()
    };
    assert_eq!(pricing.cost(&usage), Decimal::from(5));
}

#[test]
fn service_axis_uses_explicit_prices_before_multiplier() {
    let pricing = Pricing {
        input_per_million: Decimal::ONE,
        output_per_million: Decimal::from(2),
        cached_input_per_million: Some(Decimal::new(5, 1)),
        service_tier: Some("priority".into()),
        tiers: vec![
            PricingTier {
                min_prompt_tokens: 1,
                input_per_million: Some(Decimal::from(3)),
                ..Default::default()
            },
            PricingTier {
                service_tier: Some("priority".into()),
                min_prompt_tokens: 2_000_000,
                multiplier: Some(Decimal::from(2)),
                output_per_million: Some(Decimal::from(7)),
                image_output_per_million: Some(Decimal::from(11)),
                ..Default::default()
            },
        ],
        metric_rates: BTreeMap::from([("image_output_tokens".into(), Decimal::new(4, 6))]),
    };
    let mut usage = NormalizedUsage {
        input_tokens: 2_000_000,
        output_tokens: 1_000_000,
        cached_input_tokens: 1_000_000,
        ..Default::default()
    };
    usage
        .metrics
        .insert("image_output_tokens".into(), Decimal::from(1_000_000));
    assert_eq!(pricing.cost(&usage), Decimal::from(25));
}

#[test]
fn actual_serving_tier_overrides_the_requested_tier() {
    let pricing = Pricing {
        input_per_million: Decimal::ONE,
        output_per_million: Decimal::ZERO,
        cached_input_per_million: None,
        service_tier: None,
        tiers: vec![
            PricingTier {
                service_tier: Some("priority".into()),
                input_per_million: Some(Decimal::from(10)),
                ..Default::default()
            },
            PricingTier {
                service_tier: Some("standard".into()),
                input_per_million: Some(Decimal::from(4)),
                ..Default::default()
            },
        ],
        metric_rates: BTreeMap::new(),
    }
    .for_request(br#"{"service_tier":"fast"}"#);
    let usage = NormalizedUsage {
        input_tokens: 1_000_000,
        ..Default::default()
    };
    assert_eq!(pricing.cost(&usage), Decimal::from(10));

    let mut headers = HeaderMap::new();
    headers.insert("x-gemini-service-tier", HeaderValue::from_static("default"));
    let actual = response_service_tier(&headers, b"{}").expect("actual tier");
    assert_eq!(
        pricing.with_service_tier(&actual).cost(&usage),
        Decimal::from(4)
    );
}
