//! The conversation identity of one request.
//!
//! The ladder itself — the gateway header, the native client headers, the
//! native body field of the inbound shape, the conversation fingerprint — is
//! `gproxy_sdk::session`, which implements `design/session-identity.md`. It is
//! re-exported here rather than reimplemented: the rule that a Claude Code
//! request keeps Claude Code's identity whatever upstream it is routed to is
//! one rule, and two copies of it would drift the first time a client changes
//! a header.
//!
//! What this module adds is the last rung, which only the product layer can
//! supply: when nothing stable could be derived, the request's own id stands
//! in as [`SessionSource::RequestFallback`]. That is explicitly *not* a
//! session — it is unique per request, so it produces no affinity at all —
//! and saying so is better than leaving the field empty and letting each
//! consumer invent its own meaning for absence.
//!
//! A session id is never authentication. It cannot widen what a caller may
//! reach, and the affinity it feeds is scoped by the caller as well; see
//! [`super::scope`].

pub use gproxy_sdk::session::{
    GATEWAY_SESSION_HEADER, fingerprint, from_body, from_headers, strip_gateway_header,
};

use gproxy_core::{SessionIdentity, SessionSource};
use gproxy_protocol::OperationKey;
use http::HeaderMap;
use serde_json::Value;

/// The sdk's ladder, then the request-id fallback.
///
/// `agent_session_id` is the `agent_sessions` row the host has already
/// resolved for this identity, when it is a long-lived agent session whose
/// credential binding core manages as assignments rather than as affinity.
/// It is attached to whatever the ladder produced; it does not decide the id.
///
/// `None` comes back only when there is nothing to fall back to either — a
/// blank `request_id`. The result is an `Option` because
/// `RequestContext::session` is one: a host that wants no affinity at all for
/// a request passes `None`, and a session it could not derive must be
/// distinguishable from a session it chose not to have.
pub fn extract(
    headers: &HeaderMap,
    body: Option<&Value>,
    operation: OperationKey,
    request_id: &str,
    agent_session_id: Option<&str>,
) -> Option<SessionIdentity> {
    let mut identity =
        gproxy_sdk::session::extract(headers, body, operation).or_else(|| fallback(request_id))?;
    if let Some(agent) = agent_session_id.map(str::trim).filter(|id| !id.is_empty()) {
        identity.agent_session_id = Some(agent.to_owned());
    }
    Some(identity)
}

/// A request-scoped identity, which is what "no session" is recorded as.
fn fallback(request_id: &str) -> Option<SessionIdentity> {
    let id = request_id.trim();
    (!id.is_empty()).then(|| SessionIdentity {
        id: id.to_owned(),
        source: SessionSource::RequestFallback,
        field: None,
        agent_session_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_protocol::{Dialect, Operation};

    fn operation() -> OperationKey {
        OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAi,
        }
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                http::HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn nothing_stable_falls_back_to_the_request_id() {
        let identity = extract(&headers(&[]), None, operation(), "req-1", None).unwrap();
        assert_eq!(identity.id, "req-1");
        assert_eq!(identity.source, SessionSource::RequestFallback);
        assert_eq!(identity.field, None);
        assert!(!identity.is_stable());
    }

    #[test]
    fn the_gateway_header_overrides_the_clients_own() {
        let identity = extract(
            &headers(&[
                (GATEWAY_SESSION_HEADER, "ours"),
                ("thread-id", "the-clients"),
            ]),
            None,
            operation(),
            "req-1",
            None,
        )
        .unwrap();
        assert_eq!(identity.id, "ours");
        assert_eq!(identity.source, SessionSource::Gateway);
        assert!(identity.is_stable());
    }

    #[test]
    fn an_agent_session_is_attached_to_whatever_the_ladder_found() {
        let derived = extract(
            &headers(&[("thread-id", "t-1")]),
            None,
            operation(),
            "req-1",
            Some("agent-9"),
        )
        .unwrap();
        assert_eq!(derived.id, "t-1");
        assert_eq!(derived.agent_session_id.as_deref(), Some("agent-9"));
        // Including the fallback, which is still the request's own id.
        let fell_back =
            extract(&headers(&[]), None, operation(), "req-1", Some("agent-9")).unwrap();
        assert_eq!(fell_back.id, "req-1");
        assert_eq!(fell_back.agent_session_id.as_deref(), Some("agent-9"));
        // A blank one is no agent session, not an empty id.
        let blank = extract(&headers(&[]), None, operation(), "req-1", Some("  ")).unwrap();
        assert_eq!(blank.agent_session_id, None);
    }

    #[test]
    fn a_request_with_no_id_and_no_session_has_none() {
        assert!(extract(&headers(&[]), None, operation(), "   ", None).is_none());
    }

    #[test]
    fn the_gateway_header_is_scrubbed_before_forwarding() {
        let mut map = headers(&[(GATEWAY_SESSION_HEADER, "ours"), ("thread-id", "theirs")]);
        strip_gateway_header(&mut map);
        assert!(map.get(GATEWAY_SESSION_HEADER).is_none());
        assert!(map.get("thread-id").is_some());
    }
}
