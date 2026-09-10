use super::*;
use gproxy_channel_api::{QuotaQueryMode, QuotaSupport};
use rust_decimal::Decimal;
use serde_json::json;

#[test]
fn management_keys_are_explicit_and_never_replace_inference_credentials() {
    let secret = json!({"api_key":"inference-fixture", "quota_api_key":"management-fixture", "quota_team_id":"team/123"});
    for (channel, source, expected) in [
        (
            "openrouter",
            "account_balance",
            "https://openrouter.ai/api/v1/credits",
        ),
        (
            "xai",
            "prepaid_balance",
            "https://management-api.x.ai/v1/billing/teams/team%2F123/prepaid/balance",
        ),
    ] {
        let request = prepare(
            channel,
            source,
            &secret,
            &json!({"base_url":"https://inference-proxy.example"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(request.uri().to_string(), expected);
        assert_eq!(
            request.headers()["authorization"],
            "Bearer management-fixture"
        );
        assert!(
            prepare(
                channel,
                source,
                &json!({"api_key":"inference-fixture", "quota_team_id":"team"}),
                &json!({})
            )
            .is_err()
        );
    }
    let request = prepare(
        "openrouter",
        "account_balance",
        &secret,
        &json!({"quota_base_url":"https://management-proxy.example/api/v1?region=test"}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        request.uri().to_string(),
        "https://management-proxy.example/api/v1/credits?region=test"
    );
    assert!(
        prepare("openrouter", "key", &secret, &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(
        prepare("vercel", "usage_report", &secret, &json!({}))
            .unwrap()
            .is_none()
    );
}

#[test]
fn claude_uses_optional_management_override_and_preserves_original_fallback() {
    let settings = json!({"base_url":"https://inference-proxy.example/v1"});
    let original = json!({"api_key":"inference-fixture"});
    let request = prepare("claudeapi", "organization_usage", &original, &settings)
        .unwrap()
        .unwrap();
    assert_eq!(request.uri().host(), Some("inference-proxy.example"));
    assert_eq!(request.headers()["x-api-key"], "inference-fixture");
    let secret = json!({"api_key":"inference-fixture", "quota_api_key":"management-fixture"});
    let request = prepare("claudeapi", "organization_usage", &secret, &settings)
        .unwrap()
        .unwrap();
    assert_eq!(request.uri().host(), Some("api.anthropic.com"));
    assert_eq!(request.headers()["x-api-key"], "management-fixture");
    assert_eq!(request.headers()["anthropic-version"], "2023-06-01");
    assert!(!request.headers().contains_key("authorization"));
}

#[test]
fn sources_retain_independent_capabilities_and_manual_report_policy() {
    let missing = sources("openrouter", &json!({}), &json!({})).unwrap();
    assert_eq!(missing[0].id, "key");
    assert_eq!(missing[0].support, QuotaSupport::Ready);
    assert_eq!(missing[1].support, QuotaSupport::RequiresAuthorization);
    let openai = sources(
        "openai",
        &json!({"quota_api_key":"management-fixture"}),
        &json!({}),
    )
    .unwrap();
    assert_eq!(openai[0].mode, QuotaQueryMode::Response);
    assert_eq!(openai[1].support, QuotaSupport::Ready);
    assert!(!openai[1].automatic);
    assert_eq!(
        sources(
            "xai",
            &json!({"quota_api_key":"management-fixture"}),
            &json!({})
        )
        .unwrap()[0]
            .support,
        QuotaSupport::RequiresAuthorization
    );
}

#[test]
fn openrouter_and_xai_normalize_remaining_with_signed_ledger_semantics() {
    let entries = parse(
        "openrouter",
        "account_balance",
        StatusCode::OK,
        br#"{"data":{"total_credits":100.5,"total_usage":25.75}}"#,
    )
    .unwrap();
    let QuotaValue::Balance(balance) = &entries[0].value else {
        panic!()
    };
    assert_eq!(balance.remaining, Some("74.75".parse().unwrap()));
    let entries = parse(
        "xai",
        "prepaid_balance",
        StatusCode::OK,
        br#"{"total":{"val":"-1234.567"}}"#,
    )
    .unwrap();
    let QuotaValue::Balance(balance) = &entries[0].value else {
        panic!()
    };
    assert_eq!(balance.remaining, Some("12.34567".parse().unwrap()));
    assert_eq!(balance.components[0].amount, "-12.34567".parse().unwrap());
    assert_eq!(balance.availability, QuotaAvailability::Unknown);
    for (ledger, remaining) in [("-1000", "10"), ("125", "-1.25"), ("0", "0")] {
        let entries = xai::prepaid(&json!({"total":{"val":ledger}})).unwrap();
        let QuotaValue::Balance(balance) = &entries[0].value else {
            panic!()
        };
        assert_eq!(balance.remaining, Some(remaining.parse().unwrap()));
        assert_eq!(balance.availability, QuotaAvailability::Unknown);
    }
    let entries = parse(
        "xai",
        "postpaid_budget",
        StatusCode::OK,
        br#"{"spendingLimits":{"effectiveSl":{"val":"20000"},"effectiveHardSl":{"val":"22500"}}}"#,
    )
    .unwrap();
    let QuotaValue::Budget(budget) = &entries[0].value else {
        panic!()
    };
    assert_eq!(budget.limit, Some(Decimal::from(200)));
    assert_eq!(budget.used, None);
    assert_eq!(budget.remaining, None);
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::GATEWAY_TIMEOUT,
    ] {
        assert!(
            parse(
                "xai",
                "prepaid_balance",
                status,
                br#"{"total":{"val":"0"}}"#
            )
            .is_err()
        );
    }
}

#[test]
fn openai_reports_keep_currencies_and_reject_partial_pages() {
    let raw = json!({"has_more":false,"data":[{"start_time":1725148800,"end_time":1725235200,"results":[
        {"amount":{"value":0.06,"currency":"usd"}}, {"amount":{"value":0.04,"currency":"usd"}},
        {"amount":{"value":0.25,"currency":"eur"}}
    ]}]});
    let entries = openai::parse(&raw).unwrap();
    assert_eq!(entries.len(), 2);
    let QuotaValue::UsageReport(usd) = &entries
        .iter()
        .find(|e| e.id.ends_with(":USD"))
        .unwrap()
        .value
    else {
        panic!()
    };
    assert_eq!(usd.used, Decimal::new(10, 2));
    assert_eq!(usd.unit, "USD");
    let mut invalid = raw.clone();
    invalid["has_more"] = json!(true);
    assert!(openai::parse(&invalid).is_err());
    invalid["next_page"] = json!("opaque/page+1");
    let page = openai::parse_page(&invalid).unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("opaque/page+1"));
    assert_eq!(page.entries.len(), 2);
    let request = prepare_page(
        "openai",
        "organization_usage",
        &json!({"quota_api_key":"management-fixture"}),
        &json!({}),
        page.next_cursor.as_deref(),
    )
    .unwrap()
    .unwrap();
    let query: std::collections::HashMap<_, _> =
        form_urlencoded::parse(request.uri().query().unwrap().as_bytes()).collect();
    assert_eq!(query["page"], "opaque/page+1");
    invalid["has_more"] = json!(false);
    invalid["data"][0]["results"][0]["amount"]["currency"] = json!(null);
    assert!(openai::parse(&invalid).is_err());
    assert!(
        openai::parse(&json!({"has_more":false,"data":[]}))
            .unwrap()
            .is_empty()
    );
    let request = prepare(
        "openai",
        "organization_usage",
        &json!({"quota_api_key":"management-fixture"}),
        &json!({}),
    )
    .unwrap()
    .unwrap();
    let query: std::collections::HashMap<_, _> =
        form_urlencoded::parse(request.uri().query().unwrap().as_bytes()).collect();
    let start: i64 = query["start_time"].parse().unwrap();
    let end: i64 = query["end_time"].parse().unwrap();
    assert_eq!(end - start, 7 * 86400);
    assert_eq!(start % 86400, 0);
    assert_eq!(query["limit"], "7");
}

#[test]
fn opencode_go_uses_existing_key_and_keeps_zen_separate() {
    let secret = json!({"api_key":"inference-fixture"});
    let go = json!({"tier":"go"});
    let request = prepare("opencode", "go_subscription", &secret, &go)
        .unwrap()
        .unwrap();
    assert_eq!(
        request.uri().to_string(),
        "https://opencode.ai/zen/go/v1/usage"
    );
    assert_eq!(
        request.headers()["authorization"],
        "Bearer inference-fixture"
    );
    assert!(
        prepare(
            "opencode",
            "go_subscription",
            &secret,
            &json!({"tier":"zen"})
        )
        .unwrap()
        .is_none()
    );
    let explicit_base = json!({"base_url":"https://opencode.ai/zen/go/v1"});
    assert_eq!(
        sources("opencode", &secret, &explicit_base).unwrap()[0].support,
        QuotaSupport::Ready
    );
    assert!(
        prepare("opencode", "go_subscription", &secret, &explicit_base)
            .unwrap()
            .is_some()
    );
    let console = sources(
        "opencode",
        &secret,
        &json!({"base_url":"https://opencode.ai.evil.example/zen/go/v1"}),
    )
    .unwrap();
    assert_eq!(console[0].id, "console_balance");
    assert_eq!(console[0].support, QuotaSupport::RequiresAuthorization);
    let raw = json!({"usage":{
        "rolling":{"status":"ok","percent":35,"resetsAt":"2030-01-01T05:00:00Z"},
        "weekly":{"status":"rate-limited","percent":100,"resetsAt":"2030-01-07T00:00:00Z"},
        "monthly":{"status":"ok","percent":62,"resetsAt":"2030-02-01T00:00:00Z"}
    }});
    let entries = opencode::parse(&raw).unwrap();
    assert_eq!(entries.len(), 3);
    let QuotaValue::Window(window) = &entries[0].value else {
        panic!()
    };
    assert_eq!(window.used_percent, Some(Decimal::from(35)));
    assert!(window.period_end.is_some());
    assert_eq!(window.limit, None);
    assert!(opencode::parse(&json!({"usage":{}})).is_err());
    let mut invalid = raw;
    invalid["usage"]["rolling"]["percent"] = json!(101);
    assert!(opencode::parse(&invalid).is_err());
}
