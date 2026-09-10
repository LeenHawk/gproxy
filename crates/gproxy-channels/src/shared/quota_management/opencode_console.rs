#[path = "opencode_hydration.rs"]
mod hydration;

use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, QuotaAllowance, QuotaAvailability, QuotaBalance, QuotaEntry, QuotaSubject,
    QuotaValue,
};
use http::{Request, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;

pub(crate) fn prepare(secret: &Value, settings: &Value) -> Result<Request<Bytes>, ChannelError> {
    let field = super::super::quota_api::field;
    let cookie = cookie(
        secret
            .get("quota_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| ChannelError::Secret("quota_cookie missing".into()))?,
    )?;
    let workspace = field(secret, "quota_workspace_id")
        .or_else(|| field(settings, "quota_workspace_id"))
        .ok_or_else(|| ChannelError::Secret("quota_workspace_id missing".into()))?;
    let suffix = workspace.strip_prefix("wrk_").filter(|value| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    });
    if suffix.is_none() {
        return Err(ChannelError::Secret(
            "Invalid OpenCode quota_workspace_id".into(),
        ));
    }
    let base = field(secret, "quota_base_url")
        .or_else(|| field(settings, "quota_base_url"))
        .unwrap_or("https://opencode.ai");
    let path = format!(
        "/workspace/{}/billing",
        super::super::http::encode_component(workspace)
    );
    let base_uri = super::super::http::exact(base, None)?;
    if !matches!(base_uri.scheme_str(), Some("https" | "http"))
        || base_uri.query().is_some()
        || base.contains('#')
        || base_uri
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(ChannelError::Prepare(
            "Invalid OpenCode Console quota_base_url".into(),
        ));
    }
    let uri = super::super::http::join(base, &path, None)?;
    Request::get(uri)
        .header("cookie", format!("auth={cookie}"))
        .header("accept", "text/html")
        .body(Bytes::new())
        .map_err(|_| ChannelError::Prepare("Invalid OpenCode Console quota request".into()))
}

fn cookie(raw: &str) -> Result<&str, ChannelError> {
    let invalid = || ChannelError::Secret("Invalid OpenCode auth cookie".into());
    if raw.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(invalid());
    }
    let mut auth = None;
    for component in raw.split(';') {
        let Some((name, value)) = component.trim().split_once('=') else {
            continue;
        };
        if name.trim() == "auth" {
            if auth.is_some() {
                return Err(invalid());
            }
            auth = Some(value.trim());
        }
    }
    let value = match auth {
        Some(value) => value,
        None if !raw.contains(';') && !raw.contains('=') => raw.trim(),
        None => return Err(invalid()),
    };
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'"' | b',' | b';' | b'\\'))
    {
        return Err(invalid());
    }
    Ok(value)
}

pub(crate) fn parse(status: StatusCode, body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let now: i64 = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
        .try_into()
        .expect("epoch seconds fit i64");
    parse_at(status, body, now)
}

fn parse_at(status: StatusCode, body: &[u8], now: i64) -> Result<Vec<QuotaEntry>, ChannelError> {
    if !status.is_success() {
        return Err(ChannelError::Prepare(format!(
            "OpenCode Console quota returned HTTP {status}"
        )));
    }
    let html = std::str::from_utf8(body).map_err(|_| invalid())?;
    let billing = hydration::read(html)?;
    let now = time::OffsetDateTime::from_unix_timestamp(now).expect("valid epoch time");
    let start =
        time::Date::from_calendar_date(now.year(), now.month(), 1).expect("valid month start");
    let (year, month) = if now.month() == time::Month::December {
        (now.year() + 1, time::Month::January)
    } else {
        (now.year(), now.month().next())
    };
    let end = time::Date::from_calendar_date(year, month, 1).expect("valid next month start");
    let used = billing.usage.map(|usage| {
        let current = billing
            .updated
            .and_then(|date| time::OffsetDateTime::from_unix_timestamp(date).ok())
            .is_some_and(|date| date.year() == now.year() && date.month() == now.month());
        if current {
            usage / Decimal::from(100_000_000)
        } else {
            Decimal::ZERO
        }
    });
    let limit = billing.limit.filter(|value| *value != Decimal::ZERO);
    let remaining = limit
        .zip(used)
        .map(|(limit, used)| limit.checked_sub(used).ok_or_else(invalid))
        .transpose()?;
    let mut balance = super::super::quota_balances::entry(
        "console_balance",
        "console_balance:USD",
        QuotaSubject::Project,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(billing.balance / Decimal::from(100_000_000)),
            unit: Some("USD".into()),
            availability: QuotaAvailability::Unknown,
            components: vec![],
        }),
    );
    balance.label = Some("Zen workspace balance".into());
    let mut budget = super::super::quota_balances::entry(
        "console_balance",
        "console_budget:monthly",
        QuotaSubject::Project,
        QuotaValue::Budget(QuotaAllowance {
            limit,
            used,
            remaining,
            unlimited: limit.is_none(),
            unit: Some("USD".into()),
            period_start: Some(start.midnight().assume_utc().unix_timestamp()),
            period_end: Some(end.midnight().assume_utc().unix_timestamp()),
            ..Default::default()
        }),
    );
    budget.label = Some("Zen workspace monthly spending limit (UTC)".into());
    Ok(vec![balance, budget])
}

fn invalid() -> ChannelError {
    ChannelError::Prepare("Invalid or incomplete OpenCode billing hydration".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn html(values: &str) -> String {
        format!(
            r#"<script>_$HY.r["billing.get[\"wrk_fixture\"]"]=$R[15]=$R[2]($R[16]={{p:0,s:0,f:0}});</script><script>$R[22]($R[16],$R[23]={{{values}}});</script>"#
        )
    }

    #[test]
    fn console_cookie_is_isolated_and_workspace_scope_is_validated() {
        for value in [
            "fixture-token",
            "auth=fixture-token",
            "locale=en; auth=fixture-token; other=ignored",
        ] {
            let request = prepare(&json!({"quota_cookie":value,"quota_workspace_id":"wrk_fixture","api_key":"inference-fixture"}), &json!({"base_url":"https://inference.example"})).unwrap();
            assert_eq!(
                request.uri().to_string(),
                "https://opencode.ai/workspace/wrk_fixture/billing"
            );
            assert_eq!(request.headers()["cookie"], "auth=fixture-token");
            assert!(!request.headers().contains_key("authorization"));
        }
        assert!(prepare(&json!({"quota_workspace_id":"wrk_fixture"}), &json!({})).is_err());
        for workspace in [
            "",
            ".",
            "..",
            "wrk_",
            "wrk_../other",
            "other",
            "wrk_fixture?other",
        ] {
            assert!(
                prepare(
                    &json!({"quota_cookie":"fixture-token","quota_workspace_id":workspace}),
                    &json!({})
                )
                .is_err()
            );
        }
        for base in [
            "https://console.example?route=x",
            "https://user@console.example",
            "https://console.example#fragment",
        ] {
            assert!(
                prepare(
                    &json!({"quota_cookie":"fixture-token","quota_workspace_id":"wrk_fixture"}),
                    &json!({"quota_base_url":base})
                )
                .is_err()
            );
        }
        let proxy = prepare(
            &json!({"quota_cookie":"fixture-token","quota_workspace_id":"wrk_fixture"}),
            &json!({"quota_base_url":"https://console.example/proxy"}),
        )
        .unwrap();
        assert_eq!(
            proxy.uri().to_string(),
            "https://console.example/proxy/workspace/wrk_fixture/billing"
        );
        for value in [
            "auth=a;auth=b",
            "locale=en; other=x",
            "auth=",
            "a\nb",
            "auth=x\r\nother:y",
            "\nfixture-token",
        ] {
            assert!(
                prepare(
                    &json!({"quota_cookie":value,"quota_workspace_id":"wrk_fixture"}),
                    &json!({})
                )
                .is_err()
            );
        }
    }

    #[test]
    fn workspace_money_and_month_rollover_follow_official_console_units() {
        let now = 1_788_998_400; // 2026-09-10 UTC.
        let document = html(
            r#"balance:123456789,monthlyLimit:50,monthlyUsage:1250000000,timeMonthlyUsageUpdated:new Date("2026-09-09T00:00:00Z"),lite:$R[24]={balance:999}"#,
        );
        let entries = parse_at(StatusCode::OK, document.as_bytes(), now).unwrap();
        let QuotaValue::Balance(balance) = &entries[0].value else {
            panic!()
        };
        assert_eq!(balance.remaining, Some("1.23456789".parse().unwrap()));
        let QuotaValue::Budget(budget) = &entries[1].value else {
            panic!()
        };
        assert_eq!(budget.limit, Some(Decimal::from(50)));
        assert_eq!(budget.used, Some("12.5".parse().unwrap()));
        assert_eq!(budget.remaining, Some("37.5".parse().unwrap()));
        let date_ref = document
            .replace("new Date(\"2026-09-09T00:00:00Z\")", "$R[30]")
            .replace(
                "$R[22]($R[16]",
                "$R[30]=new Date(\"2026-09-09T00:00:00Z\");$R[22]($R[16]",
            );
        assert!(parse_at(StatusCode::OK, date_ref.as_bytes(), now).is_ok());
        let inline_date = document.replace("new Date(", "$R[30]=new Date(");
        assert!(parse_at(StatusCode::OK, inline_date.as_bytes(), now).is_ok());
        let previous = document.replace("2026-09-09", "2026-08-31");
        let entries = parse_at(StatusCode::OK, previous.as_bytes(), now).unwrap();
        let QuotaValue::Budget(budget) = &entries[1].value else {
            panic!()
        };
        assert_eq!(budget.used, Some(Decimal::ZERO));
        let empty =
            html("balance:0,monthlyLimit:null,monthlyUsage:null,timeMonthlyUsageUpdated:null");
        let entries = parse_at(StatusCode::OK, empty.as_bytes(), now).unwrap();
        let QuotaValue::Budget(budget) = &entries[1].value else {
            panic!()
        };
        assert!(budget.unlimited);
        assert_eq!(budget.used, None);
    }

    #[test]
    fn anchored_resolution_rejects_unrelated_balances_and_ambiguous_or_executable_fields() {
        let good = html("balance:0,monthlyLimit:0,monthlyUsage:null,timeMonthlyUsageUpdated:null");
        assert!(parse(StatusCode::OK, good.as_bytes()).is_ok());
        let whitespace = good
            .replace("=$R", " = $R ")
            .replace("($R", " ( $R ")
            .replace(",$R", " , $R ")
            .replace("={", " = { ");
        assert!(parse(StatusCode::OK, whitespace.as_bytes()).is_ok());
        let unrelated = format!(
            r#"<script>_$HY.r["payment.list[\"wrk_fixture\"]"]=$R[50]=$R[2]($R[51]={{p:0,s:0,f:0}});$R[22]($R[51],$R[52]={{balance:999}});</script>{good}"#
        );
        assert!(parse(StatusCode::OK, unrelated.as_bytes()).is_ok());

        for bad in [
            "<html><form action='/login'>Sign in</form><script>const a={balance:12}</script></html>".into(),
            html("balance:1+2,monthlyLimit:null,monthlyUsage:null,timeMonthlyUsageUpdated:null"),
            html("balance:0,balance:1,monthlyLimit:null,monthlyUsage:null,timeMonthlyUsageUpdated:null"),
            html("balance:0,monthlyLimit:null,monthlyUsage:null"),
            good.replace("$R[22]($R[16]", "$R[22]($R[99]"),
            format!("{good}{good}"),
            good.replace("balance:0", "balance:(globalThis.stolen=1)"),
        ] { assert!(parse(StatusCode::OK, bad.as_bytes()).is_err()); }
        assert!(parse(StatusCode::FORBIDDEN, good.as_bytes()).is_err());
    }
}
