//! Who owns an upstream credential, and therefore who may select it.
//!
//! This is the isolation boundary between tenants: it is what stops one
//! organization's key from spending another organization's ChatGPT
//! subscription. The answer is derived from the calling **key's binding**, not
//! from the caller's memberships — the binding is on the row, the client
//! cannot choose it, and it is the same value the budget chain and the
//! permission subject use.
//!
//! Management of a shared credential (reading its secret, editing, deleting)
//! is a different question with a different answer: that requires the `admin`
//! role of the owning scope, from [`super::MembershipIndex`]. Visibility here
//! is only about selection for a call.

use gproxy_store::entity::upstream::credential;
use std::collections::HashMap;

/// The scope a credential belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// No owner column set: usable by every caller on the instance. This is
    /// the shape a single-tenant deployment has for all of its credentials.
    Shared,
    User(String),
    Team(String),
    Org(String),
}

/// credential_id → owner.
#[derive(Clone, Debug, Default)]
pub struct CredentialOwnership {
    owners: HashMap<String, Owner>,
}

impl CredentialOwnership {
    /// The entity documents that credential management sets exactly one owner
    /// column. When a row nevertheless has several — a hand-edited database, a
    /// half-finished migration — the narrowest wins: **user > team > org**. A
    /// narrower owner can only ever reduce the set of callers that reach the
    /// credential, so a malformed row leaks less than it would under the
    /// opposite order.
    pub fn build(credentials: &[credential::Model]) -> Self {
        let mut conflicting = 0_usize;
        let owners = credentials
            .iter()
            .map(|row| {
                let set = usize::from(row.user_id.is_some())
                    + usize::from(row.team_id.is_some())
                    + usize::from(row.organization_id.is_some());
                if set > 1 {
                    conflicting += 1;
                }
                let owner = match (&row.user_id, &row.team_id, &row.organization_id) {
                    (Some(user), _, _) => Owner::User(user.clone()),
                    (None, Some(team), _) => Owner::Team(team.clone()),
                    (None, None, Some(org)) => Owner::Org(org.clone()),
                    (None, None, None) => Owner::Shared,
                };
                (row.id.clone(), owner)
            })
            .collect();
        if conflicting > 0 {
            tracing::warn!(
                count = conflicting,
                "credential rows carry more than one owner column; resolved as user > team > org"
            );
        }
        Self { owners }
    }

    pub fn owner(&self, credential_id: &str) -> Option<&Owner> {
        self.owners.get(credential_id)
    }

    /// Whether a caller bound to `caller_user` / `caller_team` / `caller_org`
    /// may select this credential.
    ///
    /// `caller_org` is the *effective* organization: for a key bound to a team
    /// it is that team's parent organization, so a team's calls reach the
    /// organization's shared credentials, which is what "usable by descendant
    /// members" means on the entity. An unknown credential id is not visible —
    /// it does not exist in this snapshot.
    pub fn visible_to(
        &self,
        credential_id: &str,
        caller_user: &str,
        caller_team: Option<&str>,
        caller_org: Option<&str>,
    ) -> bool {
        match self.owners.get(credential_id) {
            None => false,
            Some(Owner::Shared) => true,
            Some(Owner::User(owner)) => owner == caller_user,
            Some(Owner::Team(owner)) => caller_team == Some(owner.as_str()),
            Some(Owner::Org(owner)) => caller_org == Some(owner.as_str()),
        }
    }

    /// Every credential the caller may select, in id order, which is the set a
    /// call hands to the sdk as `allowed_credentials`.
    pub fn visible(
        &self,
        caller_user: &str,
        caller_team: Option<&str>,
        caller_org: Option<&str>,
    ) -> Vec<String> {
        let mut out: Vec<String> = self
            .owners
            .keys()
            .filter(|id| self.visible_to(id, caller_user, caller_team, caller_org))
            .cloned()
            .collect();
        out.sort();
        out
    }

    pub fn len(&self) -> usize {
        self.owners.len()
    }

    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_store::entity::upstream::credential::CredentialStatus;
    use serde_json::json;

    fn row(
        id: &str,
        user: Option<&str>,
        team: Option<&str>,
        org: Option<&str>,
    ) -> credential::Model {
        credential::Model {
            id: id.into(),
            provider_id: "p".into(),
            organization_id: org.map(Into::into),
            team_id: team.map(Into::into),
            user_id: user.map(Into::into),
            label: None,
            auth_kind: "api_key".into(),
            secret: Vec::new(),
            version: 0,
            connection_profile_id: None,
            proxy: None,
            metadata: json!({}),
            expires_at_ms: None,
            status: CredentialStatus::default(),
            status_reason: None,
            enabled: true,
        }
    }

    fn ownership() -> CredentialOwnership {
        CredentialOwnership::build(&[
            row("shared", None, None, None),
            row("mine", Some("alice"), None, None),
            row("theirs", Some("bob"), None, None),
            row("core-team", None, Some("core"), None),
            row("other-team", None, Some("ops"), None),
            row("acme-org", None, None, Some("acme")),
            row("other-org", None, None, Some("globex")),
        ])
    }

    #[test]
    fn owners_are_read_from_the_three_columns() {
        let ownership = ownership();
        assert_eq!(ownership.owner("shared"), Some(&Owner::Shared));
        assert_eq!(ownership.owner("mine"), Some(&Owner::User("alice".into())));
        assert_eq!(
            ownership.owner("core-team"),
            Some(&Owner::Team("core".into()))
        );
        assert_eq!(
            ownership.owner("acme-org"),
            Some(&Owner::Org("acme".into()))
        );
        assert_eq!(ownership.owner("absent"), None);
        assert_eq!(ownership.len(), 7);
    }

    #[test]
    fn a_shared_credential_is_visible_to_everyone() {
        let ownership = ownership();
        assert!(ownership.visible_to("shared", "alice", None, None));
        assert!(ownership.visible_to("shared", "bob", Some("core"), Some("acme")));
    }

    #[test]
    fn a_personal_credential_is_visible_only_to_its_owner() {
        let ownership = ownership();
        assert!(ownership.visible_to("mine", "alice", None, None));
        assert!(!ownership.visible_to("mine", "bob", None, None));
        // An organization binding does not reach into a member's own keys.
        assert!(!ownership.visible_to("mine", "bob", Some("core"), Some("acme")));
    }

    #[test]
    fn a_team_credential_needs_the_callers_key_to_be_bound_to_that_team() {
        let ownership = ownership();
        assert!(ownership.visible_to("core-team", "alice", Some("core"), Some("acme")));
        assert!(!ownership.visible_to("core-team", "alice", Some("ops"), Some("acme")));
        assert!(!ownership.visible_to("core-team", "alice", None, Some("acme")));
    }

    #[test]
    fn an_organization_credential_needs_the_callers_effective_organization() {
        let ownership = ownership();
        assert!(ownership.visible_to("acme-org", "alice", None, Some("acme")));
        // A team-bound key reaches its parent organization's credentials
        // because the caller passes the parent as the effective organization.
        assert!(ownership.visible_to("acme-org", "alice", Some("core"), Some("acme")));
        assert!(!ownership.visible_to("acme-org", "alice", None, Some("globex")));
        assert!(!ownership.visible_to("acme-org", "alice", None, None));
    }

    #[test]
    fn an_unknown_credential_is_not_visible() {
        assert!(!ownership().visible_to("absent", "alice", Some("core"), Some("acme")));
    }

    #[test]
    fn the_visible_set_is_what_a_call_may_select_from() {
        let ownership = ownership();
        assert_eq!(
            ownership.visible("alice", Some("core"), Some("acme")),
            vec!["acme-org", "core-team", "mine", "shared"]
        );
        assert_eq!(
            ownership.visible("bob", None, None),
            vec!["shared", "theirs"]
        );
        assert_eq!(
            ownership.visible("stranger", None, None),
            vec!["shared".to_string()]
        );
    }

    #[test]
    fn a_row_with_several_owners_resolves_to_the_narrowest() {
        let ownership = CredentialOwnership::build(&[
            row("a", Some("alice"), Some("core"), Some("acme")),
            row("b", None, Some("core"), Some("acme")),
        ]);
        assert_eq!(ownership.owner("a"), Some(&Owner::User("alice".into())));
        assert_eq!(ownership.owner("b"), Some(&Owner::Team("core".into())));
        assert!(!ownership.visible_to("a", "bob", Some("core"), Some("acme")));
    }
}
