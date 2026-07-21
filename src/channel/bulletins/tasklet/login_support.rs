use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;

use super::auth;
use crate::channel::ChannelError;
use crate::http::client::UpstreamClient;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignInResponse {
    Success {
        #[serde(rename = "userId")]
        user_id: String,
        #[serde(rename = "sessionToken")]
        session_token: String,
    },
    ExistingAccount,
}

#[derive(Deserialize)]
pub struct Profile {
    #[serde(rename = "userId")]
    pub user_id: String,
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

pub fn select_workspace(
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

pub async fn post_json(
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

pub fn parse_json<T: serde::de::DeserializeOwned>(
    body: Bytes,
    label: &str,
) -> Result<T, ChannelError> {
    serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Build(format!("Tasklet {label} response: {error}")))
}
