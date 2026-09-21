//! Cost of one exchange from the Store pricing tables, computed inside core
//! at settlement so budgets and the Observer see the same number.
//!
//! Rule selection follows the entity contracts: a rule fits when its provider
//! is the exchange's provider or unset, its `model_pattern` glob matches the
//! upstream model and its operation is the request's or unset. Provider rules
//! precede global rules; within one scope the lowest `(priority, id)` wins.
//! No matching rule means the exchange is *unpriced*: it costs nothing and
//! its usage carries `dimensions["unpriced"] = "true"`.
//!
//! Token kinds and where their quantities come from:
//!
//! | kind | `TokenUsage` field / `metrics` key | tier column |
//! |---|---|---|
//! | input | `input_tokens` | `input_per_million` |
//! | output | `output_tokens` minus reasoning when reasoning is priced | `output_per_million` |
//! | cache read | `cached_input_tokens` | `cache_read_per_million` |
//! | cache write 5m/30m/1h | `cache_creation_*_tokens` | `cache_creation_*_per_million` |
//! | reasoning | `reasoning_tokens` (a subset of output) | `reasoning_per_million` |
//! | image in/out | `metrics["image_input_tokens"]`, `["image_output_tokens"]` | `image_*_per_million` |
//! | audio in/cached/out | `metrics["audio_input_tokens"]`, `["cached_audio_input_tokens"]`, `["audio_output_tokens"]` | `audio_*_per_million` |
//! | video in / video | `metrics["video_input_tokens"]`, `["video_tokens"]` | `video_input_per_million`, `video_per_million` |
//!
//! A token kind's base price is the rule's `price_rates` row for the same
//! metric key (conditional rows first, matched against `usage.dimensions`).
//! The selected context tier (highest reached `min_prompt_tokens` without a
//! service tier) overrides it; the selected service tier (`actual_service_tier`)
//! either names an explicit price or multiplies the context-adjusted base by
//! its `multiplier`. Prompt length is input plus cache reads and writes. A
//! cache read without any price inherits the input price; every other kind
//! without a price is free. Every other `metrics` key (`image_outputs`,
//! `audio_seconds`, `web_searches`, ...) is charged by its rate row as
//! `amount * value / unit_quantity`, never multiplied by a service tier. A
//! `requests` rate row charges one request per priced exchange.

use gproxy_protocol::Operation;
use gproxy_seaorm::FixedDecimal;
use gproxy_store::entity::pricing::{metric, price_rate, price_rule, price_tier};
use regex::Regex;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

pub use gproxy_channel::channel::NormalizedUsage;

/// The computed charge of one exchange or request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cost {
    pub amount: Decimal,
    /// The matched rule's currency, e.g. `USD`.
    pub currency: String,
}

/// The dimension core sets on usage that matched no price rule.
pub const UNPRICED_DIMENSION: &str = "unpriced";

const MILLION: Decimal = Decimal::from_parts(1_000_000, 0, 0, false, 0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Input,
    Output,
    CacheRead,
    Cache5m,
    Cache30m,
    Cache1h,
    Reasoning,
    ImageInput,
    ImageOutput,
    AudioInput,
    CachedAudioInput,
    AudioOutput,
    VideoInput,
    Video,
}

const TOKEN_KINDS: [TokenKind; 14] = [
    TokenKind::Input,
    TokenKind::Output,
    TokenKind::CacheRead,
    TokenKind::Cache5m,
    TokenKind::Cache30m,
    TokenKind::Cache1h,
    TokenKind::Reasoning,
    TokenKind::ImageInput,
    TokenKind::ImageOutput,
    TokenKind::AudioInput,
    TokenKind::CachedAudioInput,
    TokenKind::AudioOutput,
    TokenKind::VideoInput,
    TokenKind::Video,
];

impl TokenKind {
    /// The `price_rates.metric` key and the `NormalizedUsage.metrics` key.
    fn metric(self) -> &'static str {
        match self {
            Self::Input => metric::INPUT_TOKENS,
            Self::Output => metric::OUTPUT_TOKENS,
            Self::CacheRead => metric::CACHED_INPUT_TOKENS,
            Self::Cache5m => metric::CACHE_CREATION_5M_TOKENS,
            Self::Cache30m => metric::CACHE_CREATION_30M_TOKENS,
            Self::Cache1h => metric::CACHE_CREATION_1H_TOKENS,
            Self::Reasoning => metric::REASONING_TOKENS,
            Self::ImageInput => metric::IMAGE_INPUT_TOKENS,
            Self::ImageOutput => metric::IMAGE_OUTPUT_TOKENS,
            Self::AudioInput => metric::AUDIO_INPUT_TOKENS,
            Self::CachedAudioInput => metric::CACHED_AUDIO_INPUT_TOKENS,
            Self::AudioOutput => metric::AUDIO_OUTPUT_TOKENS,
            Self::VideoInput => metric::VIDEO_INPUT_TOKENS,
            Self::Video => metric::VIDEO_TOKENS,
        }
    }

    fn quantity(self, usage: &NormalizedUsage) -> Decimal {
        let t = &usage.tokens;
        let named = match self {
            Self::Input => t.input_tokens,
            Self::Output => t.output_tokens,
            Self::CacheRead => t.cached_input_tokens,
            Self::Cache5m => t.cache_creation_5m_tokens,
            Self::Cache30m => t.cache_creation_30m_tokens,
            Self::Cache1h => t.cache_creation_1h_tokens,
            Self::Reasoning => t.reasoning_tokens,
            _ => None,
        };
        match named {
            Some(n) => Decimal::from(n),
            None => usage
                .metrics
                .get(self.metric())
                .copied()
                .unwrap_or_default(),
        }
    }

    fn tier_price(self, tier: &Tier) -> Option<Decimal> {
        tier.prices[self as usize]
    }
}

#[derive(Clone, Debug)]
struct Tier {
    id: String,
    service_tier: Option<String>,
    min_prompt_tokens: i64,
    priority: i32,
    multiplier: Option<Decimal>,
    /// Per-million prices indexed by `TokenKind`.
    prices: [Option<Decimal>; 14],
}

#[derive(Clone, Debug)]
struct Rate {
    metric: String,
    /// Price of one unit: `value / unit_quantity`.
    per_unit: Decimal,
    conditions: Option<BTreeMap<String, String>>,
}

/// One compiled `price_rules` row with its rates and tiers.
pub struct PriceRule {
    pub id: String,
    pub provider_id: Option<String>,
    pub model_pattern: String,
    pub operation: Option<Operation>,
    pub priority: i32,
    pub currency: String,
    model: Regex,
    /// Ordered by `(priority, id)`; conditional rows are selected first.
    rates: Vec<Rate>,
    tiers: Vec<Tier>,
}

/// Every enabled, valid price rule of one snapshot, ordered for lookup.
#[derive(Default)]
pub struct PriceBook {
    rules: Vec<PriceRule>,
}

fn decimal(value: FixedDecimal) -> Decimal {
    value.decimal()
}

fn condition_map(value: &serde_json::Value) -> Option<BTreeMap<String, String>> {
    let object = value.as_object()?;
    if object.is_empty() {
        return None;
    }
    object
        .iter()
        .map(|(name, value)| {
            let expected = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => return None,
            };
            Some((name.clone(), expected))
        })
        .collect()
}

impl PriceBook {
    /// Compile the enabled rules. A rule with an invalid pattern or
    /// operation, or a rate with a non-positive denominator, is skipped
    /// with a warning rather than failing the snapshot.
    pub fn compile(
        rules: &[price_rule::Model],
        rates: &[price_rate::Model],
        tiers: &[price_tier::Model],
    ) -> Self {
        let mut compiled = Vec::new();
        for rule in rules.iter().filter(|r| r.enabled) {
            let Ok(model) = crate::rewrite::glob_to_regex(&rule.model_pattern) else {
                tracing::warn!(rule = %rule.id, "price rule skipped: invalid model_pattern");
                continue;
            };
            let operation = match rule.operation.as_deref() {
                None => None,
                Some(id) => match Operation::from_id(id) {
                    Some(op) => Some(op),
                    None => {
                        tracing::warn!(rule = %rule.id, operation = id, "price rule skipped: unknown operation");
                        continue;
                    }
                },
            };
            let mut rule_rates: Vec<(i32, &str, Rate)> = Vec::new();
            for rate in rates.iter().filter(|r| r.price_rule_id == rule.id) {
                let unit = decimal(rate.unit_quantity);
                if unit <= Decimal::ZERO {
                    tracing::warn!(rule = %rule.id, rate = %rate.id, "price rate skipped: unit_quantity must be positive");
                    continue;
                }
                let conditions = match &rate.conditions {
                    None => None,
                    Some(value) => match condition_map(value) {
                        Some(map) => Some(map),
                        None => {
                            tracing::warn!(rule = %rule.id, rate = %rate.id, "price rate skipped: conditions must be a nonempty object of scalars");
                            continue;
                        }
                    },
                };
                rule_rates.push((
                    rate.priority,
                    rate.id.as_str(),
                    Rate {
                        metric: rate.metric.clone(),
                        per_unit: decimal(rate.value) / unit,
                        conditions,
                    },
                ));
            }
            rule_rates.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
            let mut rule_tiers: Vec<Tier> = tiers
                .iter()
                .filter(|t| t.price_rule_id == rule.id)
                .map(|t| Tier {
                    id: t.id.clone(),
                    service_tier: t
                        .service_tier
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_ascii_lowercase),
                    min_prompt_tokens: t.min_prompt_tokens,
                    priority: t.priority,
                    multiplier: t.multiplier.map(decimal),
                    prices: [
                        t.input_per_million.map(decimal),
                        t.output_per_million.map(decimal),
                        t.cache_read_per_million.map(decimal),
                        t.cache_creation_5m_per_million.map(decimal),
                        t.cache_creation_30m_per_million.map(decimal),
                        t.cache_creation_1h_per_million.map(decimal),
                        t.reasoning_per_million.map(decimal),
                        t.image_input_per_million.map(decimal),
                        t.image_output_per_million.map(decimal),
                        t.audio_input_per_million.map(decimal),
                        t.cached_audio_input_per_million.map(decimal),
                        t.audio_output_per_million.map(decimal),
                        t.video_input_per_million.map(decimal),
                        t.video_per_million.map(decimal),
                    ],
                })
                .collect();
            rule_tiers.sort_by(|a, b| (a.priority, &a.id).cmp(&(b.priority, &b.id)));
            compiled.push(PriceRule {
                id: rule.id.clone(),
                provider_id: rule.provider_id.clone(),
                model_pattern: rule.model_pattern.clone(),
                operation,
                priority: rule.priority,
                currency: rule.currency.clone(),
                model,
                rates: rule_rates.into_iter().map(|(_, _, r)| r).collect(),
                tiers: rule_tiers,
            });
        }
        // Provider rules first, then lowest (priority, id).
        compiled.sort_by(|a, b| {
            (a.provider_id.is_none(), a.priority, &a.id).cmp(&(
                b.provider_id.is_none(),
                b.priority,
                &b.id,
            ))
        });
        Self { rules: compiled }
    }

    pub fn rules(&self) -> &[PriceRule] {
        &self.rules
    }

    /// The rule that prices `model` for `operation` on `provider_id`.
    pub fn find(&self, provider_id: &str, model: &str, operation: Operation) -> Option<&PriceRule> {
        self.rules.iter().find(|rule| {
            rule.provider_id.as_deref().is_none_or(|p| p == provider_id)
                && rule.operation.is_none_or(|op| op == operation)
                && rule.model.is_match(model)
        })
    }

    /// Price one exchange. When the usage carries per-attempt usage, each
    /// billable attempt is priced by its own model and summed; an exchange
    /// without a model, or one no rule covers, is `None` (unpriced).
    pub fn price(
        &self,
        provider_id: &str,
        model: Option<&str>,
        operation: Operation,
        usage: &NormalizedUsage,
    ) -> Option<Cost> {
        if usage.attempts.is_empty() {
            let rule = self.find(provider_id, model?, operation)?;
            return Some(Cost {
                amount: rule.cost(usage),
                currency: rule.currency.clone(),
            });
        }
        let mut total: Option<Cost> = None;
        for attempt in &usage.attempts {
            if attempt.billable == Some(false) {
                continue;
            }
            let rule = self
                .find(provider_id, &attempt.model, operation)
                .or_else(|| self.find(provider_id, model?, operation))?;
            let amount = rule.cost(&attempt.usage);
            match &mut total {
                Some(cost) if cost.currency == rule.currency => cost.amount += amount,
                Some(_) => {}
                None => {
                    total = Some(Cost {
                        amount,
                        currency: rule.currency.clone(),
                    })
                }
            }
        }
        total.or_else(|| {
            // Every attempt was unbillable: priced, at nothing.
            self.find(provider_id, model?, operation).map(|rule| Cost {
                amount: Decimal::ZERO,
                currency: rule.currency.clone(),
            })
        })
    }
}

impl PriceRule {
    /// The rate row for `metric`: the first conditional row whose conditions
    /// all match the usage dimensions, else the first unconditional row.
    fn rate(&self, metric: &str, usage: &NormalizedUsage) -> Option<Decimal> {
        let rows = self.rates.iter().filter(|r| r.metric == metric);
        let mut fallback = None;
        for row in rows {
            match &row.conditions {
                Some(conditions) => {
                    if conditions
                        .iter()
                        .all(|(name, expected)| usage.dimensions.get(name) == Some(expected))
                    {
                        return Some(row.per_unit);
                    }
                }
                None => {
                    if fallback.is_none() {
                        fallback = Some(row.per_unit);
                    }
                }
            }
        }
        fallback
    }

    fn select_tier(&self, service_tier: Option<&str>, prompt: Decimal) -> Option<&Tier> {
        self.tiers
            .iter()
            .filter(|t| {
                t.service_tier.as_deref() == service_tier
                    && Decimal::from(t.min_prompt_tokens) <= prompt
            })
            // Highest reached threshold; the vector is already in (priority, id)
            // order so `max_by_key` on a reversed index keeps the first of equals.
            .enumerate()
            .max_by_key(|(index, t)| (t.min_prompt_tokens, std::cmp::Reverse(*index)))
            .map(|(_, t)| t)
    }

    /// The charge of `usage` under this rule, in the rule's currency.
    pub fn cost(&self, usage: &NormalizedUsage) -> Decimal {
        let modalities_included = usage
            .dimensions
            .get("token_modalities_in_totals")
            .is_some_and(|v| v == "true");
        let quantity = |kind: TokenKind| {
            let amount = kind.quantity(usage);
            if !modalities_included {
                return amount;
            }
            match kind {
                TokenKind::AudioInput => {
                    (amount - TokenKind::CachedAudioInput.quantity(usage)).max(Decimal::ZERO)
                }
                TokenKind::ImageInput => (amount
                    - usage
                        .metrics
                        .get("cached_image_input_tokens")
                        .copied()
                        .unwrap_or_default())
                .max(Decimal::ZERO),
                _ => amount,
            }
        };
        let prompt = quantity(TokenKind::Input)
            + quantity(TokenKind::CacheRead)
            + quantity(TokenKind::Cache5m)
            + quantity(TokenKind::Cache30m)
            + quantity(TokenKind::Cache1h);
        let context = self.select_tier(None, prompt);
        let service = usage
            .actual_service_tier
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase)
            .and_then(|tier| self.select_tier(Some(&tier), prompt));
        let multiplier = service.and_then(|t| t.multiplier).unwrap_or(Decimal::ONE);
        // The context-adjusted per-million base of a kind, before any service tier.
        let base = |kind: TokenKind| -> Option<Decimal> {
            context.and_then(|t| kind.tier_price(t)).or_else(|| {
                self.rate(kind.metric(), usage)
                    .map(|per_unit| per_unit * MILLION)
            })
        };
        let price = |kind: TokenKind| -> Option<Decimal> {
            if let Some(explicit) = service.and_then(|t| kind.tier_price(t)) {
                return Some(explicit);
            }
            let inherited = match kind {
                TokenKind::CacheRead => base(kind).or_else(|| base(TokenKind::Input)),
                other => base(other),
            }?;
            Some(inherited * multiplier)
        };
        let reasoning_price = price(TokenKind::Reasoning);
        let mut total = Decimal::ZERO;
        for kind in TOKEN_KINDS {
            let mut amount = quantity(kind);
            if kind == TokenKind::Output && reasoning_price.is_some() {
                amount = (amount - quantity(TokenKind::Reasoning)).max(Decimal::ZERO);
            }
            if modalities_included {
                let subsets: &[TokenKind] = match kind {
                    TokenKind::Input => &[TokenKind::AudioInput, TokenKind::ImageInput],
                    TokenKind::Output => &[TokenKind::AudioOutput, TokenKind::ImageOutput],
                    TokenKind::CacheRead => &[TokenKind::CachedAudioInput],
                    _ => &[],
                };
                for subset in subsets {
                    if price(*subset).is_some() {
                        amount = (amount - quantity(*subset)).max(Decimal::ZERO);
                    }
                }
            }
            if amount.is_zero() {
                continue;
            }
            let per_million = if kind == TokenKind::Reasoning {
                reasoning_price
            } else {
                price(kind)
            };
            if let Some(per_million) = per_million {
                total += amount * per_million / MILLION;
            }
        }
        let mut requests_seen = false;
        for (name, amount) in &usage.metrics {
            if TOKEN_KINDS.iter().any(|kind| kind.metric() == name) {
                continue;
            }
            if name == metric::REQUESTS {
                requests_seen = true;
            }
            if let Some(per_unit) = self.rate(name, usage) {
                total += *amount * per_unit;
            }
        }
        if !requests_seen && let Some(per_request) = self.rate(metric::REQUESTS, usage) {
            total += per_request;
        }
        total
    }
}

/// Price every exchange of a report in place and total the request: each
/// exchange gets its `cost` or the `unpriced` dimension; `report.cost` sums
/// the priced exchanges in the first priced exchange's currency.
pub(crate) fn price_report(
    book: &PriceBook,
    operation: Operation,
    report: &mut crate::UsageReport,
) {
    let mut total: Option<Cost> = None;
    for exchange in &mut report.exchanges {
        match book.price(
            &exchange.provider_id,
            exchange.upstream_model.as_deref(),
            operation,
            &exchange.usage,
        ) {
            Some(cost) => {
                match &mut total {
                    Some(sum) if sum.currency == cost.currency => sum.amount += cost.amount,
                    Some(_) => {}
                    None => total = Some(cost.clone()),
                }
                exchange.cost = Some(cost);
            }
            None => {
                exchange
                    .usage
                    .dimensions
                    .insert(UNPRICED_DIMENSION.to_owned(), "true".to_owned());
            }
        }
    }
    report.cost = total;
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_channel::channel::{TokenUsage, UsageAttempt};
    use gproxy_store::entity::pricing::price_unit::PriceUnit;

    fn fixed(s: &str) -> FixedDecimal {
        s.parse().unwrap()
    }
    fn rule(id: &str, provider: Option<&str>, pattern: &str, priority: i32) -> price_rule::Model {
        price_rule::Model {
            id: id.into(),
            provider_id: provider.map(Into::into),
            model_pattern: pattern.into(),
            operation: None,
            priority,
            currency: "USD".into(),
            enabled: true,
        }
    }
    fn rate(
        id: &str,
        rule: &str,
        metric: &str,
        unit: PriceUnit,
        quantity: &str,
        value: &str,
    ) -> price_rate::Model {
        price_rate::Model {
            id: id.into(),
            price_rule_id: rule.into(),
            metric: metric.into(),
            unit,
            unit_quantity: fixed(quantity),
            value: fixed(value),
            conditions: None,
            priority: 0,
        }
    }
    fn tier(id: &str, rule: &str) -> price_tier::Model {
        price_tier::Model {
            id: id.into(),
            price_rule_id: rule.into(),
            service_tier: None,
            min_prompt_tokens: 0,
            priority: 0,
            multiplier: None,
            input_per_million: None,
            output_per_million: None,
            cache_read_per_million: None,
            cache_creation_5m_per_million: None,
            cache_creation_30m_per_million: None,
            cache_creation_1h_per_million: None,
            reasoning_per_million: None,
            image_input_per_million: None,
            image_output_per_million: None,
            audio_input_per_million: None,
            cached_audio_input_per_million: None,
            audio_output_per_million: None,
            video_input_per_million: None,
            video_per_million: None,
        }
    }
    fn usage(input: u64, output: u64) -> NormalizedUsage {
        NormalizedUsage {
            tokens: TokenUsage {
                input_tokens: Some(input),
                output_tokens: Some(output),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn book() -> PriceBook {
        let rules = vec![
            rule("global", None, "gpt-*", 0),
            rule("p-gpt", Some("p"), "gpt-*", 5),
            rule("p-gpt-first", Some("p"), "gpt-4?", 1),
            rule("img", Some("p"), "image-*", 0),
        ];
        let rates = vec![
            rate(
                "r1",
                "global",
                metric::INPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "1",
            ),
            rate(
                "r2",
                "global",
                metric::OUTPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "2",
            ),
            rate(
                "r3",
                "p-gpt",
                metric::INPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "10",
            ),
            rate(
                "r4",
                "p-gpt",
                metric::OUTPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "30",
            ),
            rate(
                "r5",
                "p-gpt",
                metric::WEB_SEARCHES,
                PriceUnit::Count,
                "1000",
                "10",
            ),
            rate(
                "r6",
                "p-gpt-first",
                metric::INPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "100",
            ),
            rate(
                "r7",
                "img",
                metric::IMAGE_OUTPUTS,
                PriceUnit::Count,
                "1",
                "0.04",
            ),
            price_rate::Model {
                conditions: Some(serde_json::json!({"quality": "hd"})),
                priority: -1,
                ..rate(
                    "r8",
                    "img",
                    metric::IMAGE_OUTPUTS,
                    PriceUnit::Count,
                    "1",
                    "0.08",
                )
            },
            rate(
                "r9",
                "img",
                metric::REQUESTS,
                PriceUnit::Count,
                "1",
                "0.001",
            ),
        ];
        let tiers = vec![
            price_tier::Model {
                min_prompt_tokens: 200_000,
                input_per_million: Some(fixed("20")),
                ..tier("long", "p-gpt")
            },
            price_tier::Model {
                service_tier: Some("Priority".into()),
                multiplier: Some(fixed("2")),
                output_per_million: Some(fixed("45")),
                ..tier("priority", "p-gpt")
            },
            price_tier::Model {
                service_tier: Some("flex".into()),
                multiplier: Some(fixed("0.5")),
                ..tier("flex", "p-gpt")
            },
        ];
        PriceBook::compile(&rules, &rates, &tiers)
    }

    #[test]
    fn provider_rules_precede_global_and_lower_priority_wins() {
        let book = book();
        let op = Operation::StreamGenerateContent;
        assert_eq!(book.find("p", "gpt-5", op).unwrap().id, "p-gpt");
        assert_eq!(book.find("p", "gpt-4o", op).unwrap().id, "p-gpt-first");
        assert_eq!(book.find("other", "gpt-5", op).unwrap().id, "global");
        assert!(book.find("p", "claude-3", op).is_none());
        let cost = book
            .price("p", Some("gpt-5"), op, &usage(100_000, 100_000))
            .unwrap();
        assert_eq!(cost.amount, Decimal::from(4));
        assert_eq!(cost.currency, "USD");
        assert!(book.price("p", None, op, &usage(1, 1)).is_none());
        assert!(
            book.price("p", Some("claude-3"), op, &usage(1, 1))
                .is_none()
        );
    }

    #[test]
    fn context_and_service_tiers_override_or_multiply() {
        let book = book();
        let op = Operation::StreamGenerateContent;
        let rule = book.find("p", "gpt-5", op).unwrap();
        // Long context: input 20/M instead of 10/M.
        let mut long = usage(300_000, 0);
        assert_eq!(rule.cost(&long), Decimal::from(6));
        // Cache reads count toward the prompt and inherit the input price.
        long.tokens.input_tokens = Some(100_000);
        long.tokens.cached_input_tokens = Some(100_000);
        assert_eq!(rule.cost(&long), Decimal::from(4));
        // Priority: explicit output 45/M, inherited input doubled.
        let mut prio = usage(100_000, 1_000_000);
        prio.actual_service_tier = Some("priority".into());
        assert_eq!(rule.cost(&prio), Decimal::from(47));
        // Flex halves everything inherited.
        let mut flex = usage(100_000, 1_000_000);
        flex.actual_service_tier = Some("flex".into());
        assert_eq!(rule.cost(&flex), Decimal::from_str_exact("15.5").unwrap());
        // Unknown tier: base prices.
        let mut unknown = usage(100_000, 1_000_000);
        unknown.actual_service_tier = Some("batch".into());
        assert_eq!(rule.cost(&unknown), Decimal::from(31));
    }

    #[test]
    fn non_token_metrics_use_rate_rows_and_conditions() {
        let book = book();
        let op = Operation::CreateImage;
        let mut u = NormalizedUsage::default();
        u.metrics
            .insert(metric::IMAGE_OUTPUTS.into(), Decimal::from(2));
        let rule = book.find("p", "image-1", op).unwrap();
        // 2 * 0.04 + one request at 0.001
        assert_eq!(rule.cost(&u), Decimal::from_str_exact("0.081").unwrap());
        u.dimensions.insert("quality".into(), "hd".into());
        assert_eq!(rule.cost(&u), Decimal::from_str_exact("0.161").unwrap());
        // Per-1000 count rates.
        let mut s = usage(0, 0);
        s.metrics
            .insert(metric::WEB_SEARCHES.into(), Decimal::from(5));
        let rule = book
            .find("p", "gpt-5", Operation::StreamGenerateContent)
            .unwrap();
        assert_eq!(rule.cost(&s), Decimal::from_str_exact("0.05").unwrap());
    }

    #[test]
    fn reasoning_splits_output_and_attempts_replace_aggregate() {
        let rules = vec![rule("r", None, "o*", 0)];
        let rates = vec![
            rate(
                "a",
                "r",
                metric::OUTPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "10",
            ),
            rate(
                "b",
                "r",
                metric::REASONING_TOKENS,
                PriceUnit::Token,
                "1000000",
                "1",
            ),
        ];
        let book = PriceBook::compile(&rules, &rates, &[]);
        let op = Operation::StreamGenerateContent;
        let mut u = usage(0, 1_000_000);
        u.tokens.reasoning_tokens = Some(400_000);
        assert_eq!(
            book.find("x", "o3", op).unwrap().cost(&u),
            Decimal::from_str_exact("6.4").unwrap()
        );
        let mut aggregate = usage(0, 5_000_000);
        aggregate.attempts = vec![
            UsageAttempt {
                model: "o3".into(),
                usage: Box::new(usage(0, 1_000_000)),
                billable: None,
                started_at_ms: None,
            },
            UsageAttempt {
                model: "o3".into(),
                usage: Box::new(usage(0, 1_000_000)),
                billable: Some(false),
                started_at_ms: None,
            },
        ];
        assert_eq!(
            book.price("x", Some("o3"), op, &aggregate).unwrap().amount,
            Decimal::from(10)
        );
    }

    #[test]
    fn invalid_rows_are_skipped_not_fatal() {
        let rules = vec![
            price_rule::Model {
                operation: Some("no_such_operation".into()),
                ..rule("bad-op", None, "a*", 0)
            },
            rule("ok", None, "a*", 1),
        ];
        let rates = vec![
            rate(
                "zero",
                "ok",
                metric::INPUT_TOKENS,
                PriceUnit::Token,
                "0",
                "1",
            ),
            rate(
                "fine",
                "ok",
                metric::INPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "3",
            ),
            price_rate::Model {
                conditions: Some(serde_json::json!({"nested": {"x": 1}})),
                ..rate(
                    "cond",
                    "ok",
                    metric::OUTPUT_TOKENS,
                    PriceUnit::Token,
                    "1000000",
                    "9",
                )
            },
        ];
        let book = PriceBook::compile(&rules, &rates, &[]);
        assert_eq!(book.rules().len(), 1);
        let rule = book.find("p", "abc", Operation::GenerateContent).unwrap();
        assert_eq!(rule.id, "ok");
        assert_eq!(rule.cost(&usage(1_000_000, 1_000_000)), Decimal::from(3));
    }
    #[test]
    fn modality_subsets_do_not_charge_total_tokens_twice() {
        let rules = vec![rule("audio", None, "*", 0)];
        let rates = vec![
            rate(
                "in",
                "audio",
                metric::INPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "1",
            ),
            rate(
                "audio",
                "audio",
                metric::AUDIO_INPUT_TOKENS,
                PriceUnit::Token,
                "1000000",
                "10",
            ),
        ];
        let book = PriceBook::compile(&rules, &rates, &[]);
        let mut usage = usage(100, 0);
        usage.metrics.insert("audio_input_tokens".into(), 80.into());
        usage
            .dimensions
            .insert("token_modalities_in_totals".into(), "true".into());
        let cost = book.rules()[0].cost(&usage);
        assert_eq!(cost, Decimal::new(820, 6));
    }
}
