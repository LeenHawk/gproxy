//! A credential without `account_id` learns it from the token.
//!
//! `GET {base}/client/v4/accounts` lists the accounts the token holds a
//! permission on, in the usual Cloudflare envelope `{success, errors,
//! result: [{id, name}]}`. A token that carries only AI permissions sees an
//! empty list; it needs Account Settings Read for this. When exactly one
//! account is listed, its id is written into the secret through the refresh
//! capability, so the host persists it and later requests skip the lookup.
//! None or several accounts are refused, for this credential version only.

use super::{CloudflareAiGateway, base_url, field};
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, CredentialView, RefreshContext,
    RefreshReason,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use gproxy_protocol::capability::CapabilityFuture;
use http::{HeaderMap, HeaderValue, Method, header};
use serde_json::Value;

fn missing_account(credential: &CredentialView<'_>) -> bool {
    field(credential.secret, "account_id").is_none()
}

/// The single account id in a `/accounts` reply.
fn single_account(body: &Value) -> Result<String, ChannelError> {
    if body.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(invalid_response(
            "Cloudflare account list did not report success",
        ));
    }
    let accounts = body
        .get("result")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("Cloudflare account list has no result"))?;
    let ids: Vec<&str> = accounts
        .iter()
        .filter_map(|account| account.get("id").and_then(Value::as_str))
        .collect();
    match ids.as_slice() {
        [id] => Ok((*id).to_owned()),
        [] => Err(ChannelError::RefreshRejected(
            "the token lists no Cloudflare account; set account_id or grant Account Settings Read"
                .into(),
        )),
        _ => Err(ChannelError::RefreshRejected(format!(
            "the token lists {} Cloudflare accounts; set account_id",
            ids.len()
        ))),
    }
}

impl CredentialRefresh for CloudflareAiGateway {
    fn proactive_refresh_reason(
        &self,
        credential: CredentialView<'_>,
        _now_ms: i64,
    ) -> Option<RefreshReason> {
        missing_account(&credential).then_some(RefreshReason::ProactiveExpiry)
    }

    /// A refusal concerns this secret only; editing the credential retries.
    fn persist_rejection(&self, _reason: &str) -> bool {
        false
    }

    fn supports(&self, credential: &CredentialView<'_>) -> bool {
        missing_account(credential)
    }

    fn refresh<'a>(
        &'a self,
        ctx: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>> {
        Box::pin(async move {
            let secret = ctx.credential.secret;
            let key = field(secret, "api_key").ok_or(ChannelError::InvalidCredential)?;
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {key}"))
                    .map_err(|_| ChannelError::InvalidCredential)?,
            );
            let (status, _, body) = send(
                ctx.client,
                Method::GET,
                &format!("{}/client/v4/accounts?per_page=50", base_url(ctx.provider)),
                headers,
                None,
            )
            .await?;
            if status == http::StatusCode::UNAUTHORIZED || status == http::StatusCode::FORBIDDEN {
                return Err(ChannelError::RefreshRejected(format!(
                    "Cloudflare refused the account lookup ({status})"
                )));
            }
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(error.to_string()))?;
            let account = single_account(&payload)?;
            let mut secret = secret.clone();
            let object = secret
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert("account_id".into(), Value::String(account));
            Ok(CredentialUpdate {
                secret,
                metadata: None,
                expires_at_ms: ctx.credential.expires_at_ms,
            })
        })
    }
}
