use std::sync::Arc;

use serde_json::{Value, json};

use super::{login_social as social, login_support as support};
use crate::channel::{AuthCodeStart, ChannelError};
use crate::http::client::UpstreamClient;

const LOGIN_URL: &str = "https://tasklet.ai/login";

pub async fn start(
    client: &Arc<dyn UpstreamClient>,
    params: &Value,
    state: &str,
) -> Result<AuthCodeStart, ChannelError> {
    let method = params
        .get("auth_method")
        .and_then(Value::as_str)
        .unwrap_or("email");
    let mut extra = login_extra(method, params);
    let authorize_url = match method {
        "email" => {
            let email = params
                .get("email")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| value.contains('@') && !value.chars().any(char::is_whitespace))
                .ok_or_else(|| {
                    ChannelError::Build("Tasklet login requires a valid email".into())
                })?;
            let magic_link_secret = crate::util::rand::uuid_v4();
            support::post_json(
                client,
                "/api/auth/magic-link/request",
                &json!({"email":email,"magicLinkSecret":magic_link_secret}),
                None,
            )
            .await?;
            extra["magic_link_secret"] = Value::String(magic_link_secret);
            LOGIN_URL.into()
        }
        "google" | "microsoft" => {
            let (url, callback_state) = social::authorize_url(method, state);
            extra["callback_state"] = Value::String(callback_state);
            url
        }
        _ => {
            return Err(ChannelError::Build(
                "unsupported Tasklet login method".into(),
            ));
        }
    };
    Ok(AuthCodeStart {
        authorize_url,
        redirect_uri: if method == "email" {
            String::new()
        } else {
            social::CALLBACK_URL.into()
        },
        extra: Some(extra),
    })
}

pub async fn exchange(
    client: &Arc<dyn UpstreamClient>,
    input: &str,
    extra: Option<&Value>,
) -> Result<Value, ChannelError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ChannelError::Build("Tasklet login code is required".into()));
    }
    let extra = extra.ok_or_else(|| ChannelError::Build("Tasklet login expired".into()))?;
    let method = extra
        .get("auth_method")
        .and_then(Value::as_str)
        .unwrap_or("email");
    let sign_in_body = match method {
        "email" => {
            let magic_link_secret = extra
                .get("magic_link_secret")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ChannelError::Build("Tasklet login state is invalid".into()))?;
            json!({
                "type":"magic_link",
                "magicLinkSecret":magic_link_secret,
                "pin":input,
                "attributionHistory":[],
                "allowDuplicate":false,
            })
        }
        "google" | "microsoft" => {
            let callback_state = extra
                .get("callback_state")
                .and_then(Value::as_str)
                .ok_or_else(|| ChannelError::Build("Tasklet login state is invalid".into()))?;
            let code = social::callback_code(input, callback_state)?;
            json!({
                "type":"oauth2code",
                "provider":method,
                "code":code,
                "attributionHistory":[],
                "allowDuplicate":false,
            })
        }
        _ => {
            return Err(ChannelError::Build(
                "unsupported Tasklet login method".into(),
            ));
        }
    };
    let sign_in: support::SignInResponse = support::parse_json(
        support::post_json(client, "/api/signIn", &sign_in_body, None).await?,
        "sign-in",
    )?;
    let support::SignInResponse::Success {
        user_id,
        session_token,
    } = sign_in
    else {
        return Err(ChannelError::Build(
            "Tasklet account uses another sign-in method".into(),
        ));
    };
    if user_id.is_empty() || session_token.is_empty() {
        return Err(ChannelError::Build(
            "Tasklet sign-in response is incomplete".into(),
        ));
    }

    let profile: support::Profile = support::parse_json(
        support::post_json(client, "/api/profile", &Value::Null, Some(&session_token)).await?,
        "profile",
    )?;
    if profile.user_id != user_id {
        return Err(ChannelError::Build(
            "Tasklet profile does not match the signed-in user".into(),
        ));
    }
    let requested = extra.get("workspace_id").and_then(Value::as_str);
    let (organization_id, workspace_id) = support::select_workspace(&profile, requested)?;
    Ok(json!({
        "session_token":session_token,
        "workspace_id":workspace_id,
        "organization_id":organization_id,
    }))
}

fn login_extra(method: &str, params: &Value) -> Value {
    let mut extra = json!({"auth_method":method});
    if let Some(workspace_id) = params
        .get("workspace_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        extra["workspace_id"] = Value::String(workspace_id.to_owned());
    }
    extra
}
