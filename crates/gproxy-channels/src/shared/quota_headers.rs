use gproxy_channel_api::{QuotaAllowance, QuotaEntry, QuotaScope, QuotaSubject, QuotaValue};
use http::HeaderMap;
use rust_decimal::Decimal;

pub(crate) fn parse(channel: &str, headers: &HeaderMap, model: &str) -> Vec<QuotaEntry> {
    let dimensions: &[&str] = if channel == "claudeapi" {
        &["requests", "tokens", "input-tokens", "output-tokens"]
    } else {
        &["requests", "tokens"]
    };
    dimensions
        .iter()
        .filter_map(|dimension| {
            let name = |field| {
                if channel == "claudeapi" {
                    format!("anthropic-ratelimit-{dimension}-{field}")
                } else {
                    format!("x-ratelimit-{field}-{dimension}")
                }
            };
            let decimal = |field| {
                headers
                    .get(name(field))?
                    .to_str()
                    .ok()?
                    .parse::<Decimal>()
                    .ok()
            };
            let limit = decimal("limit");
            let remaining = decimal("remaining");
            if limit.is_none() && remaining.is_none() {
                return None;
            }
            let end = headers
                .get(name("reset"))
                .and_then(|v| v.to_str().ok())
                .and_then(|reset| {
                    if channel == "claudeapi" {
                        super::quota::iso_to_unix(reset)
                    } else {
                        duration_seconds(reset).and_then(|seconds| now().checked_add(seconds))
                    }
                });
            let mut entry = super::quota_balances::entry(
                "response_limits",
                &format!("rate:{dimension}:{model}"),
                QuotaSubject::Unknown,
                QuotaValue::RateLimit(QuotaAllowance {
                    limit,
                    remaining,
                    period_end: end,
                    unit: Some(
                        if *dimension == "requests" {
                            "requests"
                        } else {
                            "tokens"
                        }
                        .into(),
                    ),
                    ..Default::default()
                }),
            );
            entry.label = Some(dimension.replace('-', " "));
            entry.model_scope = if model.is_empty() {
                QuotaScope::Unknown
            } else {
                QuotaScope::Models(vec![model.into()])
            };
            Some(entry)
        })
        .collect()
}

fn now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
        .try_into()
        .expect("epoch seconds fit i64")
}

fn duration_seconds(raw: &str) -> Option<i64> {
    let mut remaining = raw;
    let mut total = Decimal::ZERO;
    while !remaining.is_empty() {
        let end = remaining.find(|c: char| !c.is_ascii_digit() && c != '.')?;
        let amount = remaining[..end].parse::<Decimal>().ok()?;
        if amount < Decimal::ZERO {
            return None;
        }
        remaining = &remaining[end..];
        let (unit, multiplier) = if remaining.starts_with("ms") {
            ("ms", Decimal::new(1, 3))
        } else if remaining.starts_with('s') {
            ("s", Decimal::ONE)
        } else if remaining.starts_with('m') {
            ("m", Decimal::from(60))
        } else if remaining.starts_with('h') {
            ("h", Decimal::from(3600))
        } else if remaining.starts_with('d') {
            ("d", Decimal::from(86400))
        } else {
            return None;
        };
        total = total.checked_add(amount.checked_mul(multiplier)?)?;
        remaining = &remaining[unit.len()..];
    }
    total.ceil().to_string().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_fractional_composite_reset_and_model_scope() {
        assert_eq!(duration_seconds("2m59.56s"), Some(180));
        assert_eq!(duration_seconds("1h2m3s500ms"), Some(3724));
        assert_eq!(duration_seconds("-1s"), None);
        assert_eq!(duration_seconds("garbage"), None);
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-limit-requests", "14400".parse().unwrap());
        headers.insert("x-ratelimit-remaining-requests", "14399".parse().unwrap());
        headers.insert("x-ratelimit-reset-requests", "2m59.56s".parse().unwrap());
        let result = parse("openai", &headers, "model-a");
        assert_eq!(
            result[0].model_scope,
            QuotaScope::Models(vec!["model-a".into()])
        );
        assert_ne!(result[0].id, parse("openai", &headers, "model-b")[0].id);
        let QuotaValue::RateLimit(value) = &result[0].value else {
            panic!()
        };
        assert!(value.period_end.unwrap() >= now() + 179);
        assert_eq!(value.remaining, Some(Decimal::from(14399)));
    }
}
