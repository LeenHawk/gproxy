use gproxy_channel_api::{
    Channel, QuotaAvailability, QuotaQueryMode, QuotaSubject, QuotaSupport, QuotaValue,
};
use http::{HeaderMap, StatusCode};
use rust_decimal::Decimal;
use serde_json::json;

#[test]
fn balance_requests_preserve_configured_host_and_normalize_version_suffix() {
    let secret = json!({"api_key":"test-token"});
    let cases: &[(&dyn Channel, &str, &str, &str)] = &[
        (
            &crate::DeepSeekChannel,
            "balance",
            "https://proxy.example/deepseek/v1/",
            "https://proxy.example/deepseek/user/balance",
        ),
        (
            &crate::DeepSeekChannel,
            "balance",
            "https://proxy.example/deepseek/v1?route=deepseek",
            "https://proxy.example/deepseek/user/balance?route=deepseek",
        ),
        (
            &crate::OpenRouterChannel,
            "key",
            "https://proxy.example/api/v1",
            "https://proxy.example/api/v1/key",
        ),
        (
            &crate::VercelChannel,
            "balance",
            "https://proxy.example/v1",
            "https://proxy.example/v1/credits",
        ),
        (
            &crate::KimiChannel,
            "balance",
            "https://proxy.example/moonshot/v1",
            "https://proxy.example/moonshot/v1/users/me/balance",
        ),
    ];
    for (channel, source, base, expected) in cases {
        let request = channel
            .prepare_quota_source(source, &secret, &json!({"base_url":base}))
            .unwrap()
            .unwrap();
        assert_eq!(request.uri().to_string(), *expected);
        assert_eq!(request.headers()["authorization"], "Bearer test-token");
        assert_eq!(request.method(), http::Method::GET);
        assert!(request.body().is_empty());
        assert!(
            channel
                .prepare_quota_source("unknown", &secret, &json!({}))
                .unwrap()
                .is_none()
        );
        assert!(
            channel
                .prepare_quota_source(source, &json!({}), &json!({"base_url":base}))
                .is_err()
        );
    }
}

#[test]
fn deepseek_preserves_multicurrency_components_and_explicit_availability() {
    let raw = json!({"is_available":false,"balance_infos":[
        {"currency":"CNY","total_balance":"10.05","granted_balance":"1.01","topped_up_balance":"9.04"},
        {"currency":"USD","total_balance":"0.123456789012345678","granted_balance":"0","topped_up_balance":"0.123456789012345678"}
    ]});
    let entries = crate::DeepSeekChannel
        .parse_quota_source(
            "balance",
            StatusCode::OK,
            &HeaderMap::new(),
            &serde_json::to_vec(&raw).unwrap(),
        )
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_ne!(entries[0].id, entries[1].id);
    let QuotaValue::Balance(balance) = &entries[1].value else {
        panic!()
    };
    assert_eq!(
        balance.remaining.unwrap().to_string(),
        "0.123456789012345678"
    );
    assert_eq!(balance.availability, QuotaAvailability::Unavailable);
    assert_eq!(balance.unit.as_deref(), Some("USD"));
    assert_eq!(balance.components.len(), 2);
    assert_eq!(entries[0].subject, QuotaSubject::Account);
}

#[test]
fn failures_never_become_zero_balance_or_successful_empty_snapshot() {
    for channel in [
        &crate::DeepSeekChannel as &dyn Channel,
        &crate::VercelChannel,
        &crate::KimiChannel,
    ] {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::PAYMENT_REQUIRED,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert!(
                channel
                    .parse_quota_source("balance", status, &HeaderMap::new(), br#"{}"#)
                    .is_err()
            );
        }
        for body in [
            br#"{}"#.as_slice(),
            br#"<html>oops</html>"#,
            br#"{"error":{"message":"invalid"}}"#,
        ] {
            assert!(
                channel
                    .parse_quota_source("balance", StatusCode::OK, &HeaderMap::new(), body)
                    .is_err()
            );
        }
    }
    assert!(
        super::quota_balances::deepseek(&json!({"is_available":true,"balance_infos":[
            {"currency":"CNY","total_balance":"1"},{"currency":"CNY","total_balance":"2"}
        ]}))
        .is_err()
    );
    assert!(
        super::quota_balances::deepseek(&json!({"is_available":true,"balance_infos":[
            {"currency":"USD","total_balance":"NaN"}
        ]}))
        .is_err()
    );
    assert!(
        super::quota_balances::moonshot(
            &json!({"code":1,"status":false,"data":{"available_balance":0}})
        )
        .is_err()
    );
}

#[test]
fn moonshot_uses_available_balance_without_adding_negative_cash() {
    let entries = super::quota_balances::moonshot(&json!({"code":0,"status":true,"data":{
        "available_balance":15,"cash_balance":-5,"voucher_balance":15
    }}))
    .unwrap();
    let QuotaValue::Balance(balance) = &entries[0].value else {
        panic!()
    };
    assert_eq!(balance.remaining, Some(Decimal::from(15)));
    assert_eq!(balance.components[1].amount, Decimal::from(-5));
    assert_eq!(balance.availability, QuotaAvailability::Available);
}

#[test]
fn openrouter_uses_current_remaining_budget_instead_of_lifetime_or_byok_usage() {
    let entries = super::quota_balances::openrouter(&json!({"data":{
        "limit":100,"limit_remaining":74.5,"limit_reset":"monthly", "usage":1000,
        "usage_monthly":20,"byok_usage_monthly":5.5,"include_byok_in_limit":true
    }}))
    .unwrap();
    let QuotaValue::Budget(value) = &entries[0].value else {
        panic!()
    };
    assert_eq!(value.used, Some("25.5".parse().unwrap()));
    assert_eq!(value.remaining, Some("74.5".parse().unwrap()));
    let unlimited = super::quota_balances::openrouter(
        &json!({"data":{"limit":null,"limit_remaining":null,"usage":18}}),
    )
    .unwrap();
    let QuotaValue::Budget(value) = &unlimited[0].value else {
        panic!()
    };
    assert!(value.unlimited);
    assert_eq!(value.used, None);
    assert_eq!(value.limit, None);
    let sources = crate::OpenRouterChannel.quota_sources(&json!({}), &json!({}));
    assert_eq!(sources[0].support, QuotaSupport::Ready);
    assert_eq!(sources[1].support, QuotaSupport::RequiresAuthorization);
    assert!(
        crate::OpenRouterChannel
            .prepare_quota_source(
                "account_balance",
                &json!({"api_key":"test-token"}),
                &json!({})
            )
            .is_err()
    );
}

#[test]
fn custom_detects_only_confirmed_siliconflow_origin_and_leaves_currency_unknown() {
    for base in [
        "https://api.siliconflow.cn",
        "https://api.siliconflow.cn/v1/",
    ] {
        let settings = json!({"base_url":base});
        assert_eq!(
            crate::CustomChannel.quota_sources(&json!({}), &settings)[0].support,
            QuotaSupport::Ready
        );
        let req = crate::CustomChannel
            .prepare_quota_source(
                "siliconflow_balance",
                &json!({"api_key":"test-token"}),
                &settings,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            req.uri().to_string(),
            "https://api.siliconflow.cn/v1/user/info"
        );
    }
    for base in [
        "https://api.siliconflow.cn.evil.example/v1",
        "https://api.siliconflow.cn@evil.example/v1",
        "http://api.siliconflow.cn",
        "https://api.siliconflow.cn/proxy",
        "https://unrelated.example/v1",
    ] {
        let settings = json!({"base_url":base});
        assert_eq!(
            crate::CustomChannel.quota_sources(&json!({}), &settings)[0].support,
            QuotaSupport::Unsupported
        );
        assert!(
            crate::CustomChannel
                .prepare_quota_source(
                    "siliconflow_balance",
                    &json!({"api_key":"test-token"}),
                    &settings
                )
                .unwrap()
                .is_none()
        );
    }
    let entries = super::quota_balances::siliconflow(
        &json!({"code":20000,"data":{"totalBalance":"9.5","balance":"1","chargeBalance":"8.5"}}),
    )
    .unwrap();
    let QuotaValue::Balance(value) = &entries[0].value else {
        panic!()
    };
    assert_eq!(value.unit, None);
    assert_eq!(value.availability, QuotaAvailability::Unknown);
}

#[test]
fn kimi_code_api_key_is_separate_from_moonshot_api_balance() {
    let secret = json!({"api_key":"test-token"});
    let settings = json!({"base_url":"https://api.kimi.com/coding/v1"});
    assert_eq!(
        crate::KimiChannel.quota_sources(&secret, &settings)[0].id,
        "subscription"
    );
    let request = crate::KimiChannel
        .prepare_quota_source("subscription", &secret, &settings)
        .unwrap()
        .unwrap();
    assert_eq!(
        request.uri().to_string(),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(request.headers()["authorization"], "Bearer test-token");
    assert!(
        crate::KimiChannel
            .prepare_quota_source("balance", &secret, &settings)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        crate::KimiChannel.quota_sources(&secret, &json!({}))[0].id,
        "balance"
    );
    let oauth = json!({"access_token":"test-token","device_id":"test-device"});
    let request = crate::KimiChannel
        .prepare_quota_source("subscription", &oauth, &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(request.headers()["x-msh-device-id"], "test-device");
}

#[test]
fn cline_reuses_login_identity_and_does_not_assume_manual_key_permissions() {
    let secret = json!({"access_token":"test-token","user_id":"user/123"});
    assert_eq!(
        crate::ClineChannel.quota_sources(&secret, &json!({}))[0].support,
        QuotaSupport::Ready
    );
    let request = crate::ClineChannel
        .prepare_quota_source("balance", &secret, &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(
        request.uri().to_string(),
        "https://api.cline.bot/api/v1/users/user%2F123/balance"
    );
    assert_eq!(
        request.headers()["authorization"],
        "Bearer workos:test-token"
    );
    let manual = crate::ClineChannel.quota_sources(&json!({"api_key":"test-token"}), &json!({}));
    assert_eq!(manual[0].support, QuotaSupport::Unsupported);
    let entries = crate::ClineChannel
        .parse_quota_source(
            "balance",
            StatusCode::OK,
            &HeaderMap::new(),
            br#"{"success":true,"data":{"balance":1.25,"userId":"user/123"}}"#,
        )
        .unwrap();
    assert_eq!(entries[0].subject, QuotaSubject::Account);
}

#[test]
fn non_polling_sources_do_not_prepare_paid_reports() {
    let sources = crate::VercelChannel.quota_sources(&json!({}), &json!({}));
    let report = sources
        .iter()
        .find(|source| source.id == "usage_report")
        .unwrap();
    assert_eq!(report.support, QuotaSupport::Unsupported);
    assert_eq!(report.mode, QuotaQueryMode::Unavailable);
    assert!(!report.automatic);
    assert!(
        crate::VercelChannel
            .prepare_quota_source("usage_report", &json!({"api_key":"test-token"}), &json!({}))
            .unwrap()
            .is_none()
    );

    for channel in [
        &crate::OpenAiChannel as &dyn Channel,
        &crate::ClaudeApiChannel,
    ] {
        let sources = channel.quota_sources(&json!({}), &json!({}));
        assert_eq!(sources[0].mode, QuotaQueryMode::Response);
        assert!(!sources[0].automatic);
    }
}
