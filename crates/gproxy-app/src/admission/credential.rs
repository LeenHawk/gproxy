//! Which upstream credentials a caller may select.
//!
//! This is the tenant isolation boundary, and it is decided by the calling
//! **key's binding** — `api_keys.organization_id` / `api_keys.team_id` — not
//! by the caller's memberships. Plan §2.4: a user who belongs to two
//! organizations holds a key bound to one of them, and that key cannot spend
//! the other organization's credentials. If membership decided it instead,
//! every key of a multi-org user would reach every org's subscriptions, and a
//! client could not be given a key that is narrower than its holder.
//!
//! The binding is on the row. The client does not send it, cannot choose it,
//! and it is the same value the budget chain and the permission subject use,
//! so what a request may see, what it may spend and who pays cannot disagree.
//!
//! An empty result is not an error here. "Permitted nothing" and "nothing
//! exists to permit" are different failures, and the second belongs to
//! resolution: the sdk answers `NoTarget` for it, which is a 404-shaped
//! configuration problem rather than a 403-shaped entitlement one. Admission
//! only records that the set came out empty.

use crate::{AppData, Caller};
use std::collections::BTreeSet;

/// The credentials of `all_credentials` this caller may select, which is the
/// set a call hands the sdk as `allowed_credentials`.
///
/// `all_credentials` is the instance's live credential set as the engine sees
/// it, so a credential that is disabled, retired or fully blocked never
/// reaches this filter; visibility narrows that set, it does not widen it.
///
/// **An instance administrator sees everything**, which is what makes the
/// console able to test any credential. Note that this is visibility for
/// *selection*; managing a shared credential (revealing its secret, editing,
/// deleting) is the `admin` role of the owning scope, answered by
/// [`MembershipIndex`](crate::snapshot::MembershipIndex).
pub fn visible_credentials(
    snapshot: &AppData,
    caller: &Caller,
    all_credentials: &BTreeSet<String>,
) -> BTreeSet<String> {
    if caller.is_instance_admin() {
        return all_credentials.clone();
    }
    // The *effective* organization: a key bound to a team reaches that team's
    // parent organization's credentials, which is what "usable by descendant
    // members" means on the entity.
    let organization = snapshot
        .effective_organization(caller.organization_id.as_deref(), caller.team_id.as_deref());
    all_credentials
        .iter()
        .filter(|id| {
            snapshot.credential_ownership.visible_to(
                id,
                &caller.user_id,
                caller.team_id.as_deref(),
                organization,
            )
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::support::caller;
    use gproxy_store::{
        IdentityData,
        entity::{
            identity::team,
            upstream::credential::{self, CredentialStatus},
        },
    };
    use serde_json::json;

    fn credential(
        id: &str,
        user: Option<&str>,
        team: Option<&str>,
        org: Option<&str>,
    ) -> credential::Model {
        credential::Model {
            id: id.into(),
            provider_id: "openai".into(),
            organization_id: org.map(Into::into),
            team_id: team.map(Into::into),
            user_id: user.map(Into::into),
            label: None,
            auth_kind: "api_key".into(),
            secret: Vec::new(),
            version: 0,
            connection_profile_id: None,
            metadata: json!({}),
            expires_at_ms: None,
            status: CredentialStatus::default(),
            status_reason: None,
            enabled: true,
        }
    }

    /// One credential of every owner kind, plus a second organization's, and
    /// the team → organization edge the effective organization needs.
    fn snapshot() -> AppData {
        let identity = IdentityData {
            teams: vec![team::Model {
                id: "core".into(),
                organization_id: "acme".into(),
                name: "core".into(),
                oauth_client_allowlist: None,
                created_at_ms: 0,
            }],
            ..IdentityData::default()
        };
        AppData::assemble_at(
            1,
            &identity,
            &[
                credential("c-shared", None, None, None),
                credential("c-alice", Some("alice"), None, None),
                credential("c-bob", Some("bob"), None, None),
                credential("c-core", None, Some("core"), None),
                credential("c-ops", None, Some("ops"), None),
                credential("c-acme", None, None, Some("acme")),
                credential("c-globex", None, None, Some("globex")),
            ],
            0,
        )
        .unwrap()
    }

    fn all() -> BTreeSet<String> {
        [
            "c-shared", "c-alice", "c-bob", "c-core", "c-ops", "c-acme", "c-globex",
        ]
        .iter()
        .map(|id| (*id).to_string())
        .collect()
    }

    fn visible(caller: &Caller) -> Vec<String> {
        visible_credentials(&snapshot(), caller, &all())
            .into_iter()
            .collect()
    }

    #[test]
    fn a_personal_key_sees_the_shared_ones_and_its_own() {
        assert_eq!(visible(&caller("alice", "user")), ["c-alice", "c-shared"]);
    }

    #[test]
    fn a_team_key_reaches_its_team_and_the_parent_organization() {
        let mut key = caller("alice", "user");
        key.team_id = Some("core".into());
        assert_eq!(
            visible(&key),
            ["c-acme", "c-alice", "c-core", "c-shared"],
            "a team binding carries its parent organization"
        );
    }

    #[test]
    fn a_second_organizations_key_cannot_spend_the_firsts() {
        // Same person, a key bound to the other organization: acme's
        // credentials are invisible, globex's are not.
        let mut key = caller("alice", "user");
        key.organization_id = Some("globex".into());
        assert_eq!(visible(&key), ["c-alice", "c-globex", "c-shared"]);
    }

    #[test]
    fn an_organization_key_does_not_reach_a_members_own_credential() {
        let mut key = caller("carol", "user");
        key.organization_id = Some("acme".into());
        assert_eq!(visible(&key), ["c-acme", "c-shared"]);
    }

    #[test]
    fn an_instance_admin_sees_every_credential() {
        let admin = caller("root", "admin");
        assert_eq!(visible_credentials(&snapshot(), &admin, &all()), all());
    }

    #[test]
    fn visibility_narrows_the_live_set_and_never_widens_it() {
        // `all_credentials` is the engine's live set; a credential missing
        // from it is not selectable however it is owned.
        let live = BTreeSet::from(["c-shared".to_string()]);
        assert_eq!(
            visible_credentials(&snapshot(), &caller("alice", "user"), &live),
            live
        );
        assert_eq!(
            visible_credentials(&snapshot(), &caller("root", "admin"), &live),
            live
        );
    }

    #[test]
    fn a_caller_with_nothing_of_their_own_still_sees_the_shared_ones() {
        assert_eq!(visible(&caller("stranger", "user")), ["c-shared"]);
        // And a caller for whom even that is gone gets an empty set, not an
        // error: the resolver reports it as having no target.
        let empty = BTreeSet::from(["c-bob".to_string()]);
        assert!(visible_credentials(&snapshot(), &caller("stranger", "user"), &empty).is_empty());
    }
}
