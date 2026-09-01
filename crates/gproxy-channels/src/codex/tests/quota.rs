use gproxy_channel_api::QuotaResetOutcome;

use crate::codex::quota::{from_headers, parse_probe, parse_probe_credits, parse_reset};

fn headers(pairs: &[(&str, &str)]) -> http::HeaderMap {
    pairs
        .iter()
        .map(|(name, value)| {
            (
                http::HeaderName::try_from(*name).unwrap(),
                http::HeaderValue::from_str(value).unwrap(),
            )
        })
        .collect()
}

#[test]
fn reads_both_windows_and_derives_period_start() {
    let observed = from_headers(&headers(&[
        ("x-codex-primary-used-percent", "37.5"),
        ("x-codex-primary-window-minutes", "300"),
        ("x-codex-primary-reset-at", "1756900000"),
        ("x-codex-secondary-used-percent", "12"),
        ("x-codex-secondary-window-minutes", "10080"),
        ("x-codex-secondary-reset-at", "1757300000"),
    ]));
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0].window_key, "primary");
    assert_eq!(observed[0].period_end, Some(1_756_900_000));
    assert_eq!(observed[0].period_start, Some(1_756_900_000 - 300 * 60));
    assert_eq!(observed[0].used_percent, Some("37.5".parse().unwrap()));
    assert_eq!(observed[1].window_key, "secondary");
    assert_eq!(observed[1].period_start, Some(1_757_300_000 - 10_080 * 60));
}

#[test]
fn reads_additional_header_families_beside_the_default() {
    let observed = from_headers(&headers(&[
        ("x-codex-primary-used-percent", "10"),
        ("x-codex-primary-reset-at", "1756900000"),
        ("x-codex-spark-primary-used-percent", "55"),
        ("x-codex-spark-primary-reset-at", "1756950000"),
        ("x-codex-spark-secondary-used-percent", "5"),
        ("x-codex-spark-secondary-reset-at", "1757300000"),
    ]));
    let keys: Vec<&str> = observed
        .iter()
        .map(|window| window.window_key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "primary",
            "additional_primary:codex_spark",
            "additional_secondary:codex_spark",
        ]
    );
}

#[test]
fn skips_missing_percent_and_empty_windows() {
    assert!(from_headers(&headers(&[("x-codex-primary-reset-at", "1756900000")])).is_empty());
    assert!(from_headers(&headers(&[("x-codex-primary-used-percent", "0")])).is_empty());
    let observed = from_headers(&headers(&[
        ("x-codex-primary-used-percent", "0"),
        ("x-codex-primary-reset-at", "1756900000"),
    ]));
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].period_start, None);
}

#[test]
fn probe_body_yields_windows_and_reset_credit_count() {
    let body = br#"{
        "rate_limit": {
            "primary_window": {
                "used_percent": 42,
                "limit_window_seconds": 18000,
                "reset_at": 1756900000
            }
        },
        "additional_rate_limits": [
            {
                "limit_name": "Spark",
                "metered_feature": "codex-spark",
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 63,
                        "limit_window_seconds": 18000,
                        "reset_at": 1756910000
                    },
                    "secondary_window": {
                        "used_percent": 7,
                        "limit_window_seconds": 604800,
                        "reset_at": 1757300000
                    }
                }
            }
        ],
        "rate_limit_reset_credits": {"available_count": 3}
    }"#;
    let windows = parse_probe(http::StatusCode::OK, body);
    let credits = parse_probe_credits(http::StatusCode::OK, body).unwrap();
    let keys: Vec<&str> = windows
        .iter()
        .map(|window| window.window_key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "primary",
            "additional_primary:codex_spark",
            "additional_secondary:codex_spark",
        ]
    );
    assert_eq!(windows[0].period_start, Some(1_756_900_000 - 18_000));
    assert_eq!(windows[1].used_percent, Some("63".parse().unwrap()));
    assert_eq!(windows[1].label.as_deref(), Some("Spark"));
    assert_eq!(credits.available_count, 3);
}

#[test]
fn credit_details_carry_the_soonest_available_expiry() {
    let body = serde_json::json!({
        "credits": [
            { "id": "c1", "status": "available", "expires_at": "2026-09-20T00:00:00Z" },
            { "id": "c2", "status": "available", "expires_at": "2026-09-10T00:00:00Z" },
            { "id": "c3", "status": "redeemed", "expires_at": "2026-09-01T00:00:00Z" }
        ],
        "available_count": 2
    });
    let credits =
        parse_probe_credits(http::StatusCode::OK, &serde_json::to_vec(&body).unwrap()).unwrap();
    assert_eq!(credits.available_count, 2);
    assert_eq!(
        credits.expires_at,
        crate::shared::quota::iso_to_unix("2026-09-10T00:00:00Z")
    );
}

#[test]
fn parses_every_reset_outcome() {
    let cases = [
        ("reset", QuotaResetOutcome::Reset),
        ("nothing_to_reset", QuotaResetOutcome::NothingToReset),
        ("no_credit", QuotaResetOutcome::NoCredit),
        ("already_redeemed", QuotaResetOutcome::AlreadyRedeemed),
    ];
    for (code, expected) in cases {
        let body = serde_json::to_vec(&serde_json::json!({
            "code": code,
            "windows_reset": 2
        }))
        .unwrap();
        let result = parse_reset(http::StatusCode::OK, &body).unwrap();
        assert_eq!(result.outcome, expected);
        assert_eq!(result.windows_reset, Some(2));
    }
    assert!(parse_reset(http::StatusCode::BAD_REQUEST, b"{}").is_none());
}
