//! Cline per-credential usage — `GET {base}/users/{id}/balance`.
//!
//! The balance endpoint is user-scoped, and Cline's own client resolves the id
//! from `/api/v1/users/me` before calling it. A single prepared request cannot
//! chain those, so this reads the `user_id` the login stored. A credential
//! created by pasting an API key has none and reports no usage.

use bytes::Bytes;
use http::{HeaderMap, Method, Request, StatusCode};
use serde_json::Value;

use super::{auth, login};
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{UsageCredits, UsageSnapshot};

pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let Some(user_id) = auth::field(secret, "user_id") else {
        return Ok(None);
    };
    let uri = join_url(
        super::base_url(settings),
        &format!(
            "/users/{}/balance",
            crate::channel::oauth::percent_encode(user_id)
        ),
        None,
    )?;
    let mut req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    auth::apply(&mut req, secret)?;
    Ok(Some(req))
}

pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let data = login::unwrap_envelope(raw.clone(), "balance").ok()?;
    let balance = data.get("balance").and_then(Value::as_f64)?;
    Some(UsageSnapshot {
        credits: Some(UsageCredits {
            has_credits: Some(balance > 0.0),
            balance: Some(format!("{balance}")),
            currency: Some("USD".into()),
            ..Default::default()
        }),
        raw,
        ..Default::default()
    })
}
