//! The caller facts a usage row is written with.
//!
//! Core never infers these from the scope — the scope is opaque to it — so
//! every historical fact a usage record, a quota window or an audit trail
//! carries about who made a request comes from here, once, at admission.

use crate::Caller;
use gproxy_core::UsageAttribution;

/// The attribution of one request.
///
/// `user_id` is always set: every caller this crate produces is a person,
/// even when it arrived as a key or a grant. `api_key_id` is absent for a
/// console/portal session.
///
/// The model is the one the **client asked for**, not the upstream model the
/// engine resolves it to. A usage row has to be readable against what the
/// caller typed — a rename or a re-route upstream must not rewrite history —
/// and core records the resolved upstream model separately.
///
/// Note for the sdk bridge: `ServiceRequest::user_id` must equal this
/// `user_id`. Core's own view of the caller reads it, and two different
/// answers to "who is this" inside one request is exactly the bug the
/// single-`Caller` rule exists to prevent.
pub fn attribution(caller: &Caller, model: Option<&str>) -> UsageAttribution {
    UsageAttribution {
        user_id: Some(caller.user_id.clone()),
        api_key_id: caller.api_key_id.clone(),
        model: model.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallerKind, admission::support::caller};

    #[test]
    fn a_key_caller_attributes_to_its_key_and_its_person() {
        let key = caller("alice", "user");
        let attribution = attribution(&key, Some("gpt-4o"));
        assert_eq!(attribution.user_id.as_deref(), Some("alice"));
        assert_eq!(attribution.api_key_id.as_deref(), Some("k1"));
        assert_eq!(attribution.model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn a_session_caller_has_no_key_and_an_operation_may_have_no_model() {
        let mut person = caller("alice", "user");
        person.api_key_id = None;
        person.kind = CallerKind::Session;
        let attribution = attribution(&person, None);
        assert_eq!(attribution.user_id.as_deref(), Some("alice"));
        assert_eq!(attribution.api_key_id, None);
        assert_eq!(attribution.model, None);
    }
}
