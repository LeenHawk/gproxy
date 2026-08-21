//! Codex credential-scoped account/control endpoints under ChatGPT `/wham`.

use bytes::Bytes;
use http::{HeaderMap, Method, Request, StatusCode};
use serde_json::Value;

use super::{token, usage};
use crate::channel::http_util::{build_request, exact_url, join_url};
use crate::channel::{ChannelError, CredentialControlOperation, CredentialControlResponse};

pub(super) fn request(
    operation: &CredentialControlOperation,
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let (key, method, path, query, body) = match operation {
        CredentialControlOperation::ListRateLimitResetCredits => (
            "rate_limit_reset_credits",
            Method::GET,
            "/wham/rate-limit-reset-credits".to_owned(),
            None,
            Bytes::new(),
        ),
        CredentialControlOperation::Account => (
            "account",
            Method::GET,
            "/wham/accounts/check".to_owned(),
            None,
            Bytes::new(),
        ),
        CredentialControlOperation::Profile => (
            "profile",
            Method::GET,
            "/wham/profiles/me".to_owned(),
            None,
            Bytes::new(),
        ),
        CredentialControlOperation::Settings => (
            "settings",
            Method::GET,
            "/wham/settings/user".to_owned(),
            None,
            Bytes::new(),
        ),
        CredentialControlOperation::ListTasks { query } => (
            "tasks",
            Method::GET,
            "/wham/tasks/list".to_owned(),
            query.clone(),
            Bytes::new(),
        ),
        CredentialControlOperation::GetTask { task_id } => (
            "tasks",
            Method::GET,
            format!(
                "/wham/tasks/{}",
                crate::channel::oauth::percent_encode(task_id)
            ),
            None,
            Bytes::new(),
        ),
        CredentialControlOperation::ListSiblingTurns { task_id, turn_id } => (
            "tasks",
            Method::GET,
            format!(
                "/wham/tasks/{}/turns/{}/sibling_turns",
                crate::channel::oauth::percent_encode(task_id),
                crate::channel::oauth::percent_encode(turn_id),
            ),
            None,
            Bytes::new(),
        ),
        CredentialControlOperation::CreateTask { body } => (
            "tasks",
            Method::POST,
            "/wham/tasks".to_owned(),
            None,
            Bytes::from(serde_json::to_vec(body).map_err(|error| {
                ChannelError::Build(format!("codex task request serialize: {error}"))
            })?),
        ),
        CredentialControlOperation::CodexRaw {
            label,
            method,
            path,
            query,
            headers,
            body,
            ..
        } => {
            let uri = join_url(&usage::backend_base(settings), path, query.as_deref())?;
            let mut req = build_request(method.clone(), uri, headers.clone(), body.clone())?;
            if *label != "remote_control_ws" {
                usage::apply_headers(&mut req, token::access_token(secret)?, secret)?;
                for name in [http::header::ACCEPT, http::header::CONTENT_TYPE] {
                    if let Some(value) = headers.get(&name) {
                        req.headers_mut().insert(name, value.clone());
                    }
                }
            }
            return Ok(Some(req));
        }
        _ => return Ok(None),
    };
    let uri = match crate::channel::settings::endpoint_by_key(settings, key, "") {
        Some(url) => exact_url(&url, query.as_deref())?,
        None => join_url(&usage::backend_base(settings), &path, query.as_deref())?,
    };
    let mut req = build_request(method, uri, HeaderMap::new(), body)?;
    usage::apply_headers(&mut req, token::access_token(secret)?, secret)?;
    Ok(Some(req))
}

pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<CredentialControlResponse> {
    status.is_success().then(|| {
        serde_json::from_slice(body)
            .ok()
            .map(CredentialControlResponse::Json)
    })?
}
