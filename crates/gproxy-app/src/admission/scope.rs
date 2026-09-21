//! The isolation scope a request executes under.
//!
//! Core treats `scope` as an opaque string and uses it for credential
//! affinity: two requests in the same scope may be steered onto the same
//! upstream credential, share a continuation, and inherit each other's
//! resource bindings. It never parses it and never infers a role from it.
//!
//! So the scope's only job is to answer "may these two requests be treated as
//! the same caller's traffic?", and the answer is per user — except for an
//! OAuth grant, which gets its own.

use crate::{Caller, CallerKind};

/// `grant:{grant_id}` for an OAuth-grant caller, `user:{user_id}` for everyone
/// else.
///
/// A grant is a third-party program acting for the user, and its traffic must
/// not be mixed with the user's own. Three concrete reasons, all of which are
/// silent corruption rather than a visible failure:
///
/// - **continuations.** A stateful conversation is pinned to the credential
///   that started it. If a grant shared the user's scope, the user's next
///   request could be handed a continuation belonging to a program they are
///   not running, and the program could be handed theirs.
/// - **resource bindings.** Uploaded files, conversations and video jobs live
///   on the upstream account that created them; a scope that spans two
///   clients lets one of them reference an id the other created.
/// - **revocation.** Revoking a grant must end its affinity with it. Sharing
///   the user's scope would leave the revoked client's bindings attached to
///   the user, outliving the grant that made them.
///
/// Every key of one user *does* share a scope. Keys are the same person's
/// credentials by construction: they were minted by that person, for
/// themselves, and a conversation started on one is theirs to continue on
/// another. Isolation between keys is credential visibility's job — the
/// binding — and duplicating it here would only fragment affinity.
pub fn render(caller: &Caller) -> String {
    match (&caller.kind, &caller.grant) {
        (CallerKind::OAuthGrant, Some(grant)) => format!("grant:{}", grant.grant_id),
        _ => format!("user:{}", caller.user_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GrantContext, admission::support::caller};

    #[test]
    fn a_key_caller_is_scoped_to_its_user() {
        assert_eq!(render(&caller("alice", "user")), "user:alice");
    }

    #[test]
    fn every_key_of_one_user_shares_that_scope() {
        let first = caller("alice", "user");
        let mut second = caller("alice", "user");
        second.api_key_id = Some("k2".into());
        second.team_id = Some("core".into());
        assert_eq!(render(&first), render(&second));
    }

    #[test]
    fn a_session_caller_is_the_same_person_as_their_keys() {
        let mut person = caller("alice", "user");
        person.api_key_id = None;
        person.kind = CallerKind::Session;
        assert_eq!(render(&person), "user:alice");
    }

    #[test]
    fn a_grant_gets_a_scope_of_its_own() {
        let mut grant = caller("alice", "user");
        grant.kind = CallerKind::OAuthGrant;
        grant.grant = Some(GrantContext {
            grant_id: "g1".into(),
            client_id: "codex".into(),
            scopes: Vec::new(),
            access_digest: [0; 32],
        });
        assert_eq!(render(&grant), "grant:g1");
        assert_ne!(render(&grant), render(&caller("alice", "user")));
    }

    #[test]
    fn two_grants_of_one_user_do_not_share_a_scope_either() {
        let context = |id: &str| GrantContext {
            grant_id: id.into(),
            client_id: "codex".into(),
            scopes: Vec::new(),
            access_digest: [0; 32],
        };
        let mut first = caller("alice", "user");
        first.kind = CallerKind::OAuthGrant;
        first.grant = Some(context("g1"));
        let mut second = first.clone();
        second.grant = Some(context("g2"));
        assert_ne!(render(&first), render(&second));
    }

    #[test]
    fn a_grant_kind_without_a_grant_falls_back_to_the_user() {
        // Cannot happen through `Authenticator`, and must not panic or mint a
        // scope containing an empty id if it ever does.
        let mut broken = caller("alice", "user");
        broken.kind = CallerKind::OAuthGrant;
        assert_eq!(render(&broken), "user:alice");
    }
}
