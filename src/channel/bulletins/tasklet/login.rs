use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

use super::auth;
use crate::channel::{AuthCodeStart, ChannelError};
use crate::http::client::UpstreamClient;

const LOGIN_URL: &str = "https://tasklet.ai/login";

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SignInResponse {
    Success {
        #[serde(rename = "userId")]
        user_id: String,
        #[serde(rename = "sessionToken")]
        session_token: String,
    },
    ExistingAccount,
}

#[derive(Deserialize)]
struct Profile {
    #[serde(rename = "userId")]
    user_id: String,
    organizations: Vec<Organization>,
}

#[derive(Deserialize)]
struct Organization {
    #[serde(rename = "organizationId")]
    organization_id: String,
    workspaces: Vec<Workspace>,
}

#[derive(Deserialize)]
struct Workspace {
    #[serde(rename = "workspaceId")]
    workspace_id: String,
    #[serde(rename = "type")]
    kind: String,
}

pub async fn start(
    client: &Arc<dyn UpstreamClient>,
    params: &Value,
) -> Result<AuthCodeStart, ChannelError> {
    let email = params
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.contains('@') && !value.chars().any(char::is_whitespace))
        .ok_or_else(|| ChannelError::Build("Tasklet login requires a valid email".into()))?;
    let magic_link_secret = crate::util::rand::uuid_v4();
    post_json(
        client,
        "/api/auth/magic-link/request",
        &json!({"email":email,"magicLinkSecret":magic_link_secret}),
        None,
    )
    .await?;

    let mut extra = json!({"magic_link_secret":magic_link_secret});
    if let Some(workspace_id) = params
        .get("workspace_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        extra["workspace_id"] = Value::String(workspace_id.to_owned());
    }
    Ok(AuthCodeStart {
        authorize_url: LOGIN_URL.into(),
        redirect_uri: String::new(),
        extra: Some(extra),
    })
}

pub async fn exchange(
    client: &Arc<dyn UpstreamClient>,
    pin: &str,
    extra: Option<&Value>,
) -> Result<Value, ChannelError> {
    let pin = pin.trim();
    if pin.is_empty() {
        return Err(ChannelError::Build("Tasklet PIN is required".into()));
    }
    let extra = extra.ok_or_else(|| ChannelError::Build("Tasklet login expired".into()))?;
    let magic_link_secret = extra
        .get("magic_link_secret")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Build("Tasklet login state is invalid".into()))?;
    let sign_in: SignInResponse = parse_json(
        post_json(
            client,
            "/api/signIn",
            &json!({
                "type":"magic_link",
                "magicLinkSecret":magic_link_secret,
                "pin":pin,
                "attributionHistory":[],
                "allowDuplicate":false,
            }),
            None,
        )
        .await?,
        "sign-in",
    )?;
    let SignInResponse::Success {
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

    let profile: Profile = parse_json(
        post_json(client, "/api/profile", &Value::Null, Some(&session_token)).await?,
        "profile",
    )?;
    if profile.user_id != user_id {
        return Err(ChannelError::Build(
            "Tasklet profile does not match the signed-in user".into(),
        ));
    }
    let requested = extra.get("workspace_id").and_then(Value::as_str);
    let (organization_id, workspace_id) = select_workspace(&profile, requested)?;
    Ok(json!({
        "session_token":session_token,
        "workspace_id":workspace_id,
        "organization_id":organization_id,
    }))
}

fn select_workspace(
    profile: &Profile,
    requested: Option<&str>,
) -> Result<(String, String), ChannelError> {
    let all = profile.organizations.iter().flat_map(|organization| {
        organization
            .workspaces
            .iter()
            .map(move |workspace| (organization, workspace))
    });
    let selected = if let Some(requested) = requested {
        all.clone()
            .find(|(_, workspace)| workspace.workspace_id == requested)
    } else {
        all.clone()
            .find(|(_, workspace)| workspace.kind == "personal")
            .or_else(|| all.into_iter().next())
    }
    .ok_or_else(|| ChannelError::Build("Tasklet profile has no usable workspace".into()))?;
    Ok((
        selected.0.organization_id.clone(),
        selected.1.workspace_id.clone(),
    ))
}

async fn post_json(
    client: &Arc<dyn UpstreamClient>,
    path: &str,
    body: &Value,
    token: Option<&str>,
) -> Result<Bytes, ChannelError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|error| ChannelError::Build(format!("Tasklet login payload: {error}")))?;
    let mut request = Request::post(format!("{}{path}", auth::DEFAULT_BASE_URL))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json")
        .header(header::ORIGIN, "https://tasklet.ai")
        .body(Bytes::from(bytes))
        .map_err(|error| ChannelError::Build(format!("Tasklet login request: {error}")))?;
    if let Some(token) = token {
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {token}")
                .parse()
                .map_err(|_| ChannelError::Build("Tasklet session token is invalid".into()))?,
        );
    }
    let response = client
        .send(request)
        .await
        .map_err(|error| ChannelError::Build(format!("Tasklet login request failed: {error}")))?;
    if response.status() != StatusCode::OK {
        return Err(ChannelError::Build(format!(
            "Tasklet login endpoint returned {}",
            response.status()
        )));
    }
    Ok(response.into_body())
}

fn parse_json<T: serde::de::DeserializeOwned>(body: Bytes, label: &str) -> Result<T, ChannelError> {
    serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Build(format!("Tasklet {label} response: {error}")))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use http::Response;

    use super::*;
    use crate::http::client::ClientError;

    struct MockClient {
        requests: Mutex<Vec<Request<Bytes>>>,
        responses: Mutex<VecDeque<Response<Bytes>>>,
    }

    #[async_trait]
    impl UpstreamClient for MockClient {
        async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
            self.requests.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| ClientError::Transport("missing response".into()))
        }
    }

    #[tokio::test]
    async fn email_pin_login_selects_personal_workspace() {
        let client = Arc::new(MockClient {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(VecDeque::from([
                Response::new(Bytes::from_static(br#"{"expiresAt":999999}"#)),
                Response::new(Bytes::from_static(
                    br#"{"type":"success","userId":"u_test","sessionToken":"st_test","email":"user@example.com"}"#,
                )),
                Response::new(Bytes::from_static(
                    br#"{"userId":"u_test","email":"user@example.com","organizations":[{"organizationId":"org_test","workspaces":[{"workspaceId":"ws_shared","type":"shared"},{"workspaceId":"ws_personal","type":"personal"}]}]}"#,
                )),
            ])),
        });
        let client_dyn: Arc<dyn UpstreamClient> = client.clone();

        let started = start(&client_dyn, &json!({"email":"user@example.com"}))
            .await
            .unwrap();
        let secret = exchange(&client_dyn, "123456", started.extra.as_ref())
            .await
            .unwrap();

        assert_eq!(secret["session_token"], "st_test");
        assert_eq!(secret["workspace_id"], "ws_personal");
        assert_eq!(secret["organization_id"], "org_test");
        assert!(secret.get("user_email").is_none());
        let requests = client.requests.lock().unwrap();
        assert_eq!(requests[0].uri().path(), "/api/auth/magic-link/request");
        assert_eq!(requests[1].uri().path(), "/api/signIn");
        assert_eq!(requests[2].uri().path(), "/api/profile");
        assert_eq!(
            requests[2].headers()[header::AUTHORIZATION],
            "Bearer st_test"
        );
    }
}
