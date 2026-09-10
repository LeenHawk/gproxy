use super::*;
use gproxy_channel_api::{QuotaScope, QuotaValue};
use serde_json::json;

#[test]
fn cloud_identities_use_available_management_auth_without_mixing_key_pairs() {
    for channel in [
        "vertex",
        "aistudio",
        "vertexexpress",
        "azure",
        "aws-bedrock",
        "dashscope",
    ] {
        assert_eq!(
            sources(channel, &json!({"api_key":"inference-only"}), &json!({})).unwrap()[0].support,
            QuotaSupport::RequiresAuthorization
        );
    }
    let identity = json!({"access_key_id":"local-id", "secret_access_key":"local-secret", "session_token":"local-session"});
    let request = prepare_page(
        "aws-bedrock",
        "management_quota",
        &identity,
        &json!({"region":"cn-north-1"}),
        Some("cursor+next"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        request.uri().host(),
        Some("servicequotas.cn-north-1.amazonaws.com.cn")
    );
    assert!(
        request.headers()["authorization"]
            .to_str()
            .unwrap()
            .contains("/cn-north-1/servicequotas/aws4_request")
    );
    assert_eq!(request.headers()["x-amz-security-token"], "local-session");
    assert_eq!(
        serde_json::from_slice::<Value>(request.body()).unwrap()["NextToken"],
        "cursor+next"
    );
    let partial = json!({"quota_access_key_id":"new-id", "access_key_id":"old-id", "secret_access_key":"old-secret"});
    assert!(aws::identity(&partial).is_none());
    assert!(aliyun::identity(&partial).is_none());
    let cf = json!({"api_key":"local-token", "account_id":"account-1"});
    assert_eq!(
        sources("cloudflare-ai-gateway", &cf, &json!({})).unwrap()[0].support,
        QuotaSupport::Ready
    );
    let request = prepare("cloudflare-ai-gateway", "balance", &cf, &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(
        request.uri().path(),
        "/client/v4/accounts/account-1/ai-gateway/billing/credit-balance"
    );
}

#[test]
fn cloud_pagination_preserves_scope_and_rejects_cross_origin_azure_links() {
    assert!(
        prepare(
            "aistudio",
            "management_quota",
            &json!({"quota_api_key":"local-token", "quota_project_id":".."}),
            &json!({})
        )
        .is_err()
    );
    let google = json!({"quota_api_key":"local-token", "quota_project_id":"123"});
    let request = prepare_page(
        "aistudio",
        "management_quota",
        &google,
        &json!({}),
        Some("opaque +/&"),
    )
    .unwrap()
    .unwrap();
    assert!(
        request
            .uri()
            .path()
            .contains("/projects/123/services/generativelanguage.googleapis.com/")
    );
    let query = form_urlencoded::parse(request.uri().query().unwrap().as_bytes())
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(query["pageToken"], "opaque +/&");
    let azure = json!({"quota_api_key":"local-token", "quota_subscription_id":"subscription-1", "quota_region":"westus"});
    let first = prepare("azure", "management_quota", &azure, &json!({}))
        .unwrap()
        .unwrap();
    let next = format!("{}&$skiptoken=next", first.uri());
    assert_eq!(
        prepare_page("azure", "management_quota", &azure, &json!({}), Some(&next))
            .unwrap()
            .unwrap()
            .uri()
            .to_string(),
        next
    );
    for unsafe_next in [
        next.replace("management.azure.com", "untrusted.invalid"),
        next.replace("subscription-1", "subscription-2"),
        next.replace("https://", "http://"),
    ] {
        assert!(
            prepare_page(
                "azure",
                "management_quota",
                &azure,
                &json!({}),
                Some(&unsafe_next)
            )
            .is_err()
        );
    }
}

#[test]
fn management_limits_are_not_usage_or_currency_balances() {
    let raw = json!({"metrics":[{"displayName":"Requests", "consumerQuotaLimits":[{"name":"projects/123/limits/request", "unit":"1/min/{project}/{model}", "quotaBuckets":[
        {"effectiveLimit":"100", "dimensions":{"model":"m1", "region":"r1"}},
        {"effectiveLimit":"-1", "dimensions":{"model":"m1", "region":"r2"}}
    ]}]}], "nextPageToken":"next"});
    let page = parse_page(
        "vertex",
        "management_quota",
        StatusCode::OK,
        raw.to_string().as_bytes(),
    )
    .unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("next"));
    assert_ne!(page.entries[0].id, page.entries[1].id);
    assert_eq!(
        page.entries[0].model_scope,
        QuotaScope::Models(vec!["m1".into()])
    );
    let QuotaValue::RateLimit(first) = &page.entries[0].value else {
        panic!()
    };
    assert_eq!(first.limit, Some(100.into()));
    assert!(first.used.is_none() && first.remaining.is_none());
    let QuotaValue::RateLimit(second) = &page.entries[1].value else {
        panic!()
    };
    assert!(second.unlimited && second.limit.is_none());
    let aws = json!({"Quotas":[{"QuotaArn":"arn:aws:servicequotas:us-east-1:123:bedrock/L-1", "QuotaCode":"L-1", "QuotaName":"Request capacity", "Value":123.5, "Unit":"Count"}], "NextToken":"next"});
    let page = parse_page(
        "aws-bedrock",
        "management_quota",
        StatusCode::OK,
        aws.to_string().as_bytes(),
    )
    .unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("next"));
    assert_eq!(page.entries.len(), 1);
    let azure = json!({"value":[{"name":{"value":"AccountCount"},"currentValue":3,"limit":200,"unit":"Count"}],"nextLink":"https://management.azure.com/next"});
    let page = parse_page(
        "azure",
        "management_quota",
        StatusCode::OK,
        azure.to_string().as_bytes(),
    )
    .unwrap();
    let QuotaValue::RateLimit(value) = &page.entries[0].value else {
        panic!()
    };
    assert_eq!(value.remaining, Some(197.into()));
}

#[test]
fn cloud_balances_preserve_currency_and_explicit_error_semantics() {
    let ali = json!({"Success":true,"Data":{"AvailableAmount":"123.456789012345", "Currency":"CNY", "AvailableCashAmount":"100.00", "CreditAmount":"23.456789012345"}});
    let result = parse(
        "dashscope",
        "balance",
        StatusCode::OK,
        ali.to_string().as_bytes(),
    )
    .unwrap();
    let QuotaValue::Balance(balance) = &result[0].value else {
        panic!()
    };
    assert_eq!(balance.remaining.unwrap().to_string(), "123.456789012345");
    assert_eq!(balance.components.len(), 2);
    assert_eq!(balance.unit.as_deref(), Some("CNY"));
    let cf = json!({"success":true,"errors":[],"result":{"balance":-2.5}});
    let result = parse(
        "cloudflare-ai-gateway",
        "balance",
        StatusCode::OK,
        cf.to_string().as_bytes(),
    )
    .unwrap();
    let QuotaValue::Balance(balance) = &result[0].value else {
        panic!()
    };
    assert!(balance.unit.is_none());
    assert_eq!(
        balance.availability,
        gproxy_channel_api::QuotaAvailability::Unknown
    );
    assert!(
        parse(
            "cloudflare-ai-gateway",
            "balance",
            StatusCode::OK,
            br#"{"success":false,"result":{"balance":0}}"#
        )
        .is_err()
    );
    assert!(
        parse(
            "dashscope",
            "balance",
            StatusCode::OK,
            br#"{"Success":false,"Data":{"AvailableAmount":"0"}}"#
        )
        .is_err()
    );
    let request = prepare("dashscope", "balance", &json!({"quota_access_key_id":"local-id", "quota_access_key_secret":"local-secret", "quota_session_token":"local-session"}), &json!({})).unwrap().unwrap();
    assert_eq!(request.headers()["x-acs-action"], "QueryAccountBalance");
    assert_eq!(request.headers()["x-acs-security-token"], "local-session");
    assert!(
        request.headers()["authorization"]
            .to_str()
            .unwrap()
            .contains("x-acs-security-token")
    );
}
