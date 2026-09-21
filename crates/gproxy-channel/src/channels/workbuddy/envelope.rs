//! The gateway envelope every WorkBuddy reply arrives in.
//!
//! `copilot.tencent.com` wraps its answers as `{code, msg, data}` with `code`
//! zero for success; the plugin reads `data` and treats anything else as an
//! error with a code (v3 `workbuddy/{shape,login,refresh}.rs`). A login also
//! uses two of those codes as "keep polling", which is why the envelope is
//! parsed rather than judged by HTTP status alone.

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde::Deserialize;
use serde_json::{Map, Value};

/// The envelope's own success code.
pub(super) const OK: i64 = 0;
/// "The browser has not finished authorizing yet" (v3 `login.rs`).
pub(super) const AUTH_PENDING: i64 = 11217;
/// "The account is not readable yet", the same answer one call later.
pub(super) const ACCOUNT_PENDING: i64 = 12151;

#[derive(Deserialize)]
pub(super) struct Envelope<T> {
    #[serde(default)]
    pub(super) code: i64,
    #[serde(default, alias = "message")]
    pub(super) msg: String,
    #[serde(default = "Option::default")]
    pub(super) data: Option<T>,
}

impl<T> Envelope<T> {
    /// The payload, or the envelope's own error.
    pub(super) fn data(self, what: &str) -> Result<T, ChannelError> {
        if self.code != OK {
            return Err(ChannelError::InvalidResponse(format!(
                "WorkBuddy {what}: {} ({})",
                self.msg, self.code
            )));
        }
        self.data.ok_or_else(|| {
            ChannelError::InvalidResponse(format!("WorkBuddy {what}: the envelope has no data"))
        })
    }
}

pub(super) fn parse<T: serde::de::DeserializeOwned>(
    body: &[u8],
    what: &str,
) -> Result<Envelope<T>, ChannelError> {
    serde_json::from_slice(body)
        .map_err(|error| ChannelError::InvalidResponse(format!("WorkBuddy {what}: {error}")))
}

/// Lift `data` to the top level, keeping any outer field it did not name. A
/// body that is not a success envelope is returned exactly as it arrived, so
/// an upstream error reaches the client unchanged.
pub(super) fn unwrap(body: Bytes) -> Bytes {
    let Ok(Value::Object(mut outer)) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    if outer.get("code").and_then(Value::as_i64) != Some(OK) {
        return body;
    }
    let Some(Value::Object(mut inner)) = outer.remove("data") else {
        return body;
    };
    merge(&mut inner, outer);
    Bytes::from(Value::Object(inner).to_string())
}

pub(super) fn merge(inner: &mut Map<String, Value>, outer: Map<String, Value>) {
    for (name, value) in outer {
        inner.entry(name).or_insert(value);
    }
}
