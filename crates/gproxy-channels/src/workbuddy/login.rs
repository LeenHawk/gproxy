use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, ChannelError, ChannelLogin, CredentialAcquisition, DeviceInit, DevicePoll,
    DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::WorkBuddyChannel;

impl ChannelLogin for WorkBuddyChannel {
    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let url = format!(
                "{}/v2/plugin/auth/state?platform=workbuddy",
                super::auth::base_url(ctx.provider_settings)
            );
            let request = request(http::Method::POST, &url, Bytes::from_static(b"{}"))?;
            let envelope: Envelope<AuthState> =
                crate::shared::login::send_json(http, request, "WorkBuddy auth state").await?;
            let state = data(envelope, "WorkBuddy auth state")?;
            Ok(DeviceInit {
                device_code: state.state,
                user_code: "No code required".into(),
                verification_uri: state.auth_url,
                interval_secs: 1,
            })
        })
    }

    fn device_poll<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: DevicePollCtx<'a>,
    ) -> BoxFuture<'a, Result<DevicePoll, ChannelError>> {
        Box::pin(async move {
            let base = super::auth::base_url(ctx.provider_settings);
            let encoded = crate::shared::http::encode_component(ctx.device_code);
            let token_request = request(
                http::Method::GET,
                &format!("{base}/v2/plugin/auth/token?state={encoded}"),
                Bytes::new(),
            )?;
            let token_envelope: Envelope<Tokens> =
                crate::shared::login::send_json(http, token_request, "WorkBuddy auth token")
                    .await?;
            if token_envelope.code == 11217 {
                return Ok(DevicePoll::Pending);
            }
            let tokens = data(token_envelope, "WorkBuddy auth token")?;
            let mut account_request = request(
                http::Method::GET,
                &format!("{base}/v2/plugin/login/account?state={encoded}"),
                Bytes::new(),
            )?;
            super::auth::insert(
                account_request.headers_mut(),
                http::header::AUTHORIZATION,
                &format!("Bearer {}", tokens.access_token),
            )?;
            if let Some(domain) = tokens.domain.as_deref() {
                super::auth::insert(
                    account_request.headers_mut(),
                    http::header::HeaderName::from_static("x-domain"),
                    domain,
                )?;
            }
            let account_envelope: Envelope<Account> =
                crate::shared::login::send_json(http, account_request, "WorkBuddy account").await?;
            if account_envelope.code == 12151 {
                return Ok(DevicePoll::Pending);
            }
            let account = data(account_envelope, "WorkBuddy account")?;
            Ok(DevicePoll::Ready(CredentialAcquisition::oauth(secret(
                tokens, account,
            ))))
        })
    }
}

fn request(
    method: http::Method,
    url: &str,
    body: Bytes,
) -> Result<http::Request<Bytes>, ChannelError> {
    http::Request::builder()
        .method(method)
        .uri(url)
        .header(http::header::ACCEPT, "application/json")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("x-product", "SaaS")
        .body(body)
        .map_err(|error| ChannelError::Login(error.to_string()))
}

fn data<T>(envelope: Envelope<T>, what: &str) -> Result<T, ChannelError> {
    if envelope.code != 0 {
        return Err(ChannelError::Login(format!(
            "{what}: {} ({})",
            envelope.msg, envelope.code
        )));
    }
    envelope
        .data
        .ok_or_else(|| ChannelError::Login(format!("{what}: missing data")))
}

fn secret(tokens: Tokens, account: Account) -> Value {
    let now = crate::shared::login::now_ms();
    let mut value = json!({
        "access_token":tokens.access_token,
        "expires_at_ms":tokens.expires_at.unwrap_or_else(|| {
            now.saturating_add(tokens.expires_in.unwrap_or(3_600).saturating_mul(1_000))
        }),
        "user_id":account.uid,
    });
    let object = value.as_object_mut().expect("login secret is an object");
    insert(object, "refresh_token", Some(tokens.refresh_token));
    insert(object, "domain", tokens.domain);
    insert(object, "nickname", account.nickname);
    insert(object, "account_type", account.account_type);
    insert(object, "enterprise_id", account.enterprise_id);
    insert(object, "enterprise_name", account.enterprise_name);
    insert(object, "department_full_name", account.department_full_name);
    value
}

fn insert(object: &mut Map<String, Value>, name: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        object.insert(name.into(), Value::String(value));
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    code: i64,
    #[serde(default, alias = "message")]
    msg: String,
    data: Option<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthState {
    state: String,
    auth_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in: Option<i64>,
    expires_at: Option<i64>,
    domain: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    uid: String,
    nickname: Option<String>,
    #[serde(rename = "type")]
    account_type: Option<String>,
    enterprise_id: Option<String>,
    enterprise_name: Option<String>,
    department_full_name: Option<String>,
}
