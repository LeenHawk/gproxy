use std::collections::BTreeMap;

use http::{HeaderMap, HeaderValue};
use rust_decimal::Decimal;

use crate::control::{FailoverBudget, Plan, Pricing, PricingTier, response_service_tier};
use crate::usage::NormalizedUsage;

use super::memory::MemoryHost;
use super::{block_on, core, request, target};
use crate::{InitError, ResponseBody};

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
        conditional_metric_rates: BTreeMap::new(),
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
        conditional_metric_rates: BTreeMap::new(),
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
        conditional_metric_rates: BTreeMap::new(),
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

/// The composition rule from design/architecture.md, "Tiered pricing":
/// a tier's `multiplier` composes with the prompt ladder, an explicit tier
/// price replaces it. The middle case undercharges by design — an explicit
/// tier price must declare the thresholds it means to cover.
#[test]
fn a_tier_multiplier_composes_with_the_prompt_ladder_but_an_explicit_price_replaces_it() {
    let long_context = PricingTier {
        min_prompt_tokens: 200_000,
        input_per_million: Some(Decimal::from(2)),
        ..Default::default()
    };
    let usage = NormalizedUsage {
        input_tokens: 300_000,
        ..Default::default()
    };
    let priced = |batch: PricingTier| {
        Pricing {
            input_per_million: Decimal::ONE,
            output_per_million: Decimal::ZERO,
            cached_input_per_million: None,
            service_tier: Some("batch".into()),
            tiers: vec![long_context.clone(), batch],
            metric_rates: BTreeMap::new(),
            conditional_metric_rates: BTreeMap::new(),
        }
        .cost(&usage)
    };
    let rate = |cost: Decimal| cost / Decimal::new(3, 1);

    let multiplier = priced(PricingTier {
        service_tier: Some("batch".into()),
        multiplier: Some(Decimal::new(5, 1)),
        ..Default::default()
    });
    assert_eq!(rate(multiplier), Decimal::ONE);

    let explicit_without_threshold = priced(PricingTier {
        service_tier: Some("batch".into()),
        input_per_million: Some(Decimal::new(5, 1)),
        ..Default::default()
    });
    assert_eq!(rate(explicit_without_threshold), Decimal::new(5, 1));

    let cross_declared = priced(PricingTier {
        service_tier: Some("batch".into()),
        min_prompt_tokens: 200_000,
        input_per_million: Some(Decimal::ONE),
        ..Default::default()
    });
    assert_eq!(rate(cross_declared), Decimal::ONE);
}

#[test]
fn model_preprocessing_resolves_alias_then_suffix_then_route() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let mut state = host.state.lock().expect("state lock");
    state
        .aliases
        .insert("client-model".into(), "route-model-thinking-high".into());
    state.plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    drop(state);
    let core = core(&host)?;
    let mut request = request(false, "suffix-order");
    request.body =
        bytes::Bytes::from_static(br#"{"model":"client-model","input":"hi","stream":false}"#);
    block_on(core.execute(&host, request)).expect("suffix request");
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.resolved_models, [Some("route-model".into())]);
    let body: serde_json::Value =
        serde_json::from_slice(state.upstream_bodies.last().expect("upstream body")).unwrap();
    assert_eq!(body["model"], "route-model");
    assert_eq!(body["reasoning"]["effort"], "high");
    Ok(())
}

#[test]
fn tier_suffix_reaches_request_and_settlement_pricing() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let mut tier_target = target();
    tier_target.upstream_model = "tier-model".into();
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![tier_target],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let core = core(&host)?;
    let mut request = request(false, "suffix-tier");
    request.body = bytes::Bytes::from_static(
        br#"{"model":"route-model-tier-auto","input":"hi","stream":false}"#,
    );
    let outcome = block_on(core.execute(&host, request)).expect("tier suffix request");
    assert!(matches!(outcome.body, ResponseBody::Full(_)));
    let state = host.state.lock().expect("state lock");
    let body: serde_json::Value =
        serde_json::from_slice(state.upstream_bodies.last().expect("upstream body")).unwrap();
    assert_eq!(body["model"], "route-model");
    assert_eq!(body["service_tier"], "auto");
    assert_eq!(state.settlements[0].cost, Decimal::new(6, 5));
    Ok(())
}
