//! Who a request is charged to.
//!
//! The chain is the calling key's binding read top to bottom, narrowest owner
//! first: `api_key`, `user`, `subscription`, `team`, `org`. Core takes the
//! whole list and applies **every** enabled `quotas` row belonging to **any**
//! owner in it, so the order here is not precedence — all of them must pass —
//! it is the order the owners are reported in, which is what a log, an error
//! and the console read.
//!
//! The kinds are the bare strings `api_key`, `user`, `subscription`, `team`
//! and `org`. Core compares them verbatim against `quotas.owner_kind` and
//! knows no hierarchy between them, so these five spellings are a contract
//! with the rows an operator has already written: renaming one silently
//! detaches every budget that names it.

use crate::Caller;
use gproxy_core::BudgetOwner;

/// The budget owners of one caller, in order, skipping the parts that are not
/// set.
///
/// A session caller has no key, so its chain starts at `user`. An OAuth-grant
/// caller has the same chain as the key behind the grant — the grant's
/// internal key id, its user, its subscription, its team, its organization —
/// because a grant spends the account it was issued against, not a budget of
/// its own. Giving a grant its own budget owner would mean a limit that no
/// operator wrote and that disappears when the grant is revoked.
///
/// `org` is `api_keys.organization_id` exactly as stored: a key bound to a
/// team does **not** also charge that team's parent organization. Credential
/// visibility expands a team to its parent because reaching a shared
/// credential is what a team is for; a budget is a row an operator wrote
/// against one owner, and charging a second owner they did not name would
/// exhaust an organization's allowance from a team key they never associated
/// with it. An operator who wants both binds the key to both.
pub fn chain(caller: &Caller) -> Vec<BudgetOwner> {
    let mut owners = Vec::with_capacity(5);
    if let Some(api_key_id) = &caller.api_key_id {
        owners.push(BudgetOwner::new("api_key", api_key_id));
    }
    owners.push(BudgetOwner::new("user", &caller.user_id));
    if let Some(subscription_id) = &caller.subscription_id {
        owners.push(BudgetOwner::new("subscription", subscription_id));
    }
    if let Some(team_id) = &caller.team_id {
        owners.push(BudgetOwner::new("team", team_id));
    }
    if let Some(organization_id) = &caller.organization_id {
        owners.push(BudgetOwner::new("org", organization_id));
    }
    owners
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallerKind, GrantContext, admission::support::caller};

    fn rendered(caller: &Caller) -> Vec<String> {
        chain(caller).iter().map(BudgetOwner::to_string).collect()
    }

    #[test]
    fn a_personal_key_charges_the_key_and_its_user() {
        assert_eq!(
            rendered(&caller("alice", "user")),
            ["api_key:k1", "user:alice"]
        );
    }

    #[test]
    fn a_session_caller_has_no_key_to_charge() {
        let mut person = caller("alice", "user");
        person.api_key_id = None;
        person.kind = CallerKind::Session;
        assert_eq!(rendered(&person), ["user:alice"]);
    }

    #[test]
    fn a_subscription_sits_between_the_user_and_the_team() {
        let mut key = caller("alice", "user");
        key.subscription_id = Some("s1".into());
        key.team_id = Some("core".into());
        assert_eq!(
            rendered(&key),
            ["api_key:k1", "user:alice", "subscription:s1", "team:core"]
        );
    }

    #[test]
    fn a_team_key_charges_the_team_and_not_its_parent_organization() {
        let mut key = caller("alice", "user");
        key.team_id = Some("core".into());
        assert_eq!(rendered(&key), ["api_key:k1", "user:alice", "team:core"]);
    }

    #[test]
    fn the_whole_chain_is_five_owners_in_a_fixed_order() {
        let mut key = caller("alice", "user");
        key.subscription_id = Some("s1".into());
        key.team_id = Some("core".into());
        key.organization_id = Some("acme".into());
        assert_eq!(
            rendered(&key),
            [
                "api_key:k1",
                "user:alice",
                "subscription:s1",
                "team:core",
                "org:acme"
            ]
        );
    }

    #[test]
    fn a_grant_charges_exactly_what_its_key_charges() {
        let mut key = caller("alice", "user");
        key.api_key_id = Some("k-oauth".into());
        key.organization_id = Some("acme".into());
        let mut grant = key.clone();
        grant.kind = CallerKind::OAuthGrant;
        grant.grant = Some(GrantContext {
            grant_id: "g1".into(),
            client_id: "codex".into(),
            scopes: Vec::new(),
            access_digest: [0; 32],
        });
        assert_eq!(chain(&grant), chain(&key));
        assert_eq!(
            rendered(&grant),
            ["api_key:k-oauth", "user:alice", "org:acme"]
        );
    }

    #[test]
    fn the_kinds_are_the_strings_the_quota_rows_hold() {
        let mut key = caller("alice", "user");
        key.subscription_id = Some("s1".into());
        key.team_id = Some("core".into());
        key.organization_id = Some("acme".into());
        let owners = chain(&key);
        let kinds: Vec<&str> = owners.iter().map(|owner| owner.kind.as_str()).collect();
        assert_eq!(kinds, ["api_key", "user", "subscription", "team", "org"]);
    }
}
