use bytes::Bytes;
use gproxy_channel_api::SimpleHttp;
use http::header::{ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde_json::{Map, Value};

pub(super) async fn enrich(http: &dyn SimpleHttp, secret: &mut Value) {
    let Some(access_token) = secret
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    else {
        return;
    };
    let Ok(mut request) = http::Request::get(format!(
        "{}/api/oauth/profile",
        super::auth::DEFAULT_BASE_URL
    ))
    .header(AUTHORIZATION, format!("Bearer {access_token}"))
    .header(CONTENT_TYPE, "application/json")
    .header(ACCEPT, "application/json, text/plain, */*")
    .header(ACCEPT_ENCODING, "gzip, compress, deflate, br")
    .header(USER_AGENT, "axios/1.13.6")
    .body(Bytes::new()) else {
        return;
    };
    request
        .extensions_mut()
        .insert(super::profile::CLIENT_PROFILE.clone());
    let Ok(response) = http.send(request).await else {
        return;
    };
    if !response.status().is_success() {
        return;
    }
    let Ok(profile) = serde_json::from_slice::<Value>(response.body()) else {
        return;
    };
    merge(secret, &profile);
}

fn merge(secret: &mut Value, profile: &Value) {
    let Some(secret) = secret.as_object_mut() else {
        return;
    };
    let account = profile.get("account");
    insert_string(
        secret,
        "user_email",
        account.and_then(|value| value.get("email")),
    );
    insert_string(
        secret,
        "account_uuid",
        account.and_then(|value| value.get("uuid")),
    );

    let organization = profile.get("organization");
    for (key, value) in [
        ("organization_uuid", "uuid"),
        ("organization_type", "organization_type"),
        ("rate_limit_tier", "rate_limit_tier"),
        ("seat_tier", "seat_tier"),
        ("billing_type", "billing_type"),
    ] {
        insert_string(
            secret,
            key,
            organization.and_then(|organization| organization.get(value)),
        );
    }
    if let Some(enabled) = organization
        .and_then(|value| value.get("has_extra_usage_enabled"))
        .and_then(Value::as_bool)
    {
        secret.insert("has_extra_usage_enabled".into(), Value::Bool(enabled));
    }
}

fn insert_string(secret: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    let Some(value) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    secret.insert(key.into(), Value::String(value.into()));
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use bytes::Bytes;
    use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
    use serde_json::json;

    #[test]
    fn fetches_and_merges_account_identity_and_plan() {
        let mut secret = json!({"access_token": "token"});
        ready(super::enrich(&ProfileHttp, &mut secret));

        assert_eq!(secret["user_email"], "user@example.com");
        assert_eq!(secret["account_uuid"], "acct-1");
        assert_eq!(secret["organization_uuid"], "org-1");
        assert_eq!(secret["rate_limit_tier"], "default_claude_max_20x");
        assert_eq!(secret["has_extra_usage_enabled"], true);
    }

    struct ProfileHttp;

    impl SimpleHttp for ProfileHttp {
        fn send<'a>(
            &'a self,
            request: http::Request<Bytes>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
            assert_eq!(request.uri(), "https://api.anthropic.com/api/oauth/profile");
            assert_eq!(
                request.headers()[http::header::AUTHORIZATION],
                "Bearer token"
            );
            assert_eq!(request.headers()[http::header::USER_AGENT], "axios/1.13.6");
            assert_eq!(
                request
                    .extensions()
                    .get::<gproxy_channel_api::ClientProfile>(),
                Some(&super::super::profile::CLIENT_PROFILE)
            );
            Box::pin(async {
                Ok(http::Response::new(Bytes::from_static(
                    br#"{"account":{"uuid":"acct-1","email":"user@example.com"},"organization":{"uuid":"org-1","organization_type":"claude_max","rate_limit_tier":"default_claude_max_20x","seat_tier":"max_20x","has_extra_usage_enabled":true,"billing_type":"stripe_subscription"}}"#,
                )))
            })
        }
    }

    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }
}
