use bytes::Bytes;
use gproxy_channel_api::{AuthCodeStart, ChannelError, ClientProfile, SimpleHttp};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde_json::{Value, json};

pub(crate) struct GoogleLogin {
    pub client_id: &'static str,
    pub client_secret: &'static str,
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub redirect_uri: &'static str,
    pub scope: &'static str,
    pub code_assist_base: &'static str,
    pub fallback_tier: &'static str,
    pub user_agent: &'static str,
    pub metadata: fn(Option<&str>) -> Value,
    pub profile: &'static ClientProfile,
}

pub(crate) fn start(
    config: &GoogleLogin,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
    params: &Value,
) -> AuthCodeStart {
    let redirect_uri = if redirect_uri.trim().is_empty() {
        config.redirect_uri
    } else {
        redirect_uri
    };
    let query = super::http::form(&[
        ("response_type", "code"),
        ("client_id", config.client_id),
        ("redirect_uri", redirect_uri),
        ("scope", config.scope),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge),
        ("state", state),
    ]);
    AuthCodeStart {
        authorize_url: format!("{}?{query}", config.authorize_url),
        redirect_uri: redirect_uri.into(),
        extra: project_hint(params).map(|project_id| json!({ "project_id":project_id })),
    }
}

pub(crate) async fn exchange(
    http: &dyn SimpleHttp,
    config: &GoogleLogin,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    extra: Option<&Value>,
) -> Result<Value, ChannelError> {
    let mut request = super::login::form_request(
        http::Method::POST,
        config.token_url,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", config.client_id),
            ("client_secret", config.client_secret),
            ("code_verifier", verifier),
        ],
    )?;
    request.extensions_mut().insert(config.profile.clone());
    let token: Value = super::login::send_json(http, request, "Google token").await?;
    let access_token = super::login::field(&token, "access_token")
        .ok_or_else(|| ChannelError::Login("Google token missing access_token".into()))?
        .to_owned();
    let hint = extra.and_then(project_hint);
    let (project_id, tier) = resolve_project(http, config, &access_token, hint).await?;
    let mut secret = json!({
        "access_token":access_token,
        "expires_at_ms":super::login::now_ms().saturating_add(
            token.get("expires_in").and_then(Value::as_i64).unwrap_or(3_600)
                .max(0).saturating_mul(1_000),
        ),
        "project_id":project_id,
        "client_id":config.client_id,
        "client_secret":config.client_secret,
        "oauth_token_url":config.token_url,
    });
    if let Some(refresh) = super::login::field(&token, "refresh_token") {
        secret["refresh_token"] = Value::String(refresh.into());
    }
    if let Some(tier) = tier {
        secret["rate_limit_tier"] = Value::String(tier);
    }
    if let Some(email) = user_email(http, config.profile, &access_token).await {
        secret["user_email"] = Value::String(email);
    }
    Ok(secret)
}

async fn resolve_project(
    http: &dyn SimpleHttp,
    config: &GoogleLogin,
    access_token: &str,
    hint: Option<&str>,
) -> Result<(String, Option<String>), ChannelError> {
    let metadata = (config.metadata)(hint);
    let mut load = json!({ "metadata":metadata });
    if let Some(project) = hint {
        load["cloudaicompanionProject"] = Value::String(project.into());
    }
    let loaded = code_assist_post(
        http,
        config,
        access_token,
        "/v1internal:loadCodeAssist",
        &load,
    )
    .await?;
    let tier = subscription_tier(&loaded);
    if let Some(project) = loaded.get("cloudaicompanionProject").and_then(project_id) {
        return Ok((project, tier));
    }
    let tier_id = default_tier(&loaded).unwrap_or(config.fallback_tier);
    let mut onboard = json!({ "tierId":tier_id, "metadata":metadata });
    if let Some(project) = hint {
        onboard["cloudaicompanionProject"] = Value::String(project.into());
    }
    let mut onboarded = code_assist_post(
        http,
        config,
        access_token,
        "/v1internal:onboardUser",
        &onboard,
    )
    .await?;
    if onboarded.get("done").and_then(Value::as_bool) == Some(false)
        && let Some(name) = super::login::field(&onboarded, "name").map(str::to_owned)
    {
        onboarded = poll_operation(http, config, access_token, &name).await?;
    }
    let project = onboarded
        .get("response")
        .and_then(|value| value.get("cloudaicompanionProject"))
        .and_then(project_id)
        .or_else(|| {
            onboarded
                .get("cloudaicompanionProject")
                .and_then(project_id)
        })
        .or_else(|| hint.map(str::to_owned))
        .ok_or_else(|| {
            ChannelError::Login(
                "Code Assist returned no project; retry login or provide project_id".into(),
            )
        })?;
    Ok((project, tier))
}

async fn poll_operation(
    http: &dyn SimpleHttp,
    config: &GoogleLogin,
    access_token: &str,
    name: &str,
) -> Result<Value, ChannelError> {
    for _ in 0..60 {
        http.wait(std::time::Duration::from_secs(5)).await;
        let path = format!("/v1internal/{}", name.trim_start_matches('/'));
        let mut request = http::Request::get(format!(
            "{}{path}",
            config.code_assist_base.trim_end_matches('/')
        ))
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, config.user_agent)
        .body(Bytes::new())
        .map_err(|error| ChannelError::Login(error.to_string()))?;
        request.extensions_mut().insert(config.profile.clone());
        let operation: Value =
            super::login::send_json(http, request, "Code Assist operation").await?;
        if operation.get("done").and_then(Value::as_bool) != Some(false) {
            return Ok(operation);
        }
    }
    Err(ChannelError::Login(
        "Code Assist onboarding timed out waiting for project".into(),
    ))
}

async fn code_assist_post(
    http: &dyn SimpleHttp,
    config: &GoogleLogin,
    access_token: &str,
    path: &str,
    body: &Value,
) -> Result<Value, ChannelError> {
    let mut request = super::login::json_request(
        http::Method::POST,
        &format!("{}{path}", config.code_assist_base.trim_end_matches('/')),
        body,
    )?;
    request.headers_mut().insert(
        AUTHORIZATION,
        format!("Bearer {access_token}")
            .parse()
            .map_err(|_| ChannelError::Login("invalid Google access token".into()))?,
    );
    request.headers_mut().insert(
        USER_AGENT,
        config
            .user_agent
            .parse()
            .map_err(|_| ChannelError::Login("invalid Google user agent".into()))?,
    );
    request.extensions_mut().insert(config.profile.clone());
    super::login::send_json(http, request, "Code Assist").await
}

async fn user_email(
    http: &dyn SimpleHttp,
    profile: &'static ClientProfile,
    access_token: &str,
) -> Option<String> {
    let mut request = http::Request::get("https://www.googleapis.com/oauth2/v1/userinfo?alt=json")
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .body(Bytes::new())
        .ok()?;
    request.extensions_mut().insert(profile.clone());
    let response = http.send(request).await.ok()?;
    let value: Value = serde_json::from_slice(response.body()).ok()?;
    super::login::field(&value, "email").map(str::to_owned)
}

fn project_hint(value: &Value) -> Option<&str> {
    super::login::field(value, "project_id")
}

fn project_id(value: &Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn default_tier(value: &Value) -> Option<&str> {
    value
        .get("allowedTiers")?
        .as_array()?
        .iter()
        .find_map(|tier| {
            (tier.get("isDefault").and_then(Value::as_bool) == Some(true))
                .then(|| super::login::field(tier, "id"))
                .flatten()
        })
}

fn subscription_tier(value: &Value) -> Option<String> {
    let raw = ["paidTier", "currentTier"].into_iter().find_map(|name| {
        value
            .get(name)
            .and_then(|tier| super::login::field(tier, "id"))
    })?;
    Some(match raw.to_ascii_lowercase().as_str() {
        "g1-ultra-tier" | "ws-ai-ultra-business-tier" => "ultra".into(),
        "free-tier" => "free".into(),
        _ => "pro".into(),
    })
}
