//! Which OAuth clients a user may complete an authorization with.
//!
//! The policy is an intersection over levels: global (the `settings` row), the
//! user's organizations, their teams, and the user. A level that configures
//! nothing inherits — it does not restrict. A level that configures something
//! must admit the client, and within one level the configured rows are a
//! union, so being in any one admitting organization is enough for that level.
//! `Some([])` therefore denies: the level is configured and admits nobody.
//!
//! **This is enforced twice, on purpose.** `gproxy-store`'s
//! `operations::oauth_policy::allowed` builds the same condition in SQL so
//! that `operations/oauth.rs` (`issue_many`, `exchange_tokens_many`,
//! `allowed_many`) evaluates the policy in the same statement as the write it
//! protects — a policy change concurrent with an authorization cannot slip
//! between a check and an insert. The copy here is for the fast path: refusing
//! an authorization request, listing clients, showing the portal, all without
//! a round trip. The two must agree, and the SQL one is the authority. If this
//! one is ever more permissive, the write still fails; if it is ever more
//! restrictive, a client is refused early. Neither is a security hole, but
//! both are bugs.
//!
//! One level is missing here and cannot be added: the **global** allowlist
//! lives on `settings`, which is in `ControlData`, not `IdentityData`. Callers
//! that need the complete answer take it from the store.

use gproxy_store::entity::{
    identity::{organization, team, user},
    oauth,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The configured allowlists of one snapshot. Only rows that restrict are
/// held; an absent entry is an unrestricted level.
#[derive(Clone, Debug, Default)]
pub struct ClientAllowlist {
    users: HashMap<String, HashSet<String>>,
    organizations: HashMap<String, HashSet<String>>,
    teams: HashMap<String, HashSet<String>>,
}

impl ClientAllowlist {
    pub fn build(
        users: &[user::Model],
        organizations: &[organization::Model],
        teams: &[team::Model],
    ) -> Self {
        Self {
            users: collect(
                users
                    .iter()
                    .map(|row| (&row.id, &row.oauth_client_allowlist)),
            ),
            organizations: collect(
                organizations
                    .iter()
                    .map(|row| (&row.id, &row.oauth_client_allowlist)),
            ),
            teams: collect(
                teams
                    .iter()
                    .map(|row| (&row.id, &row.oauth_client_allowlist)),
            ),
        }
    }

    /// Whether `client_id` may be used by this user.
    ///
    /// `organizations` must be the user's *effective* organizations — the ones
    /// joined directly plus the parents of the teams joined — because that is
    /// what the store's query evaluates. [`super::MembershipIndex::
    /// effective_organizations`] produces exactly that list.
    ///
    /// The global level is not checked here; see the module documentation.
    pub fn allowed(
        &self,
        user_id: &str,
        organizations: &[String],
        teams: &[String],
        client_id: &str,
    ) -> bool {
        level(
            self.organizations
                .iter()
                .filter(|(id, _)| organizations.iter().any(|want| want == *id))
                .map(|(_, allowed)| allowed),
            client_id,
        ) && level(
            self.teams
                .iter()
                .filter(|(id, _)| teams.iter().any(|want| want == *id))
                .map(|(_, allowed)| allowed),
            client_id,
        ) && level(self.users.get(user_id).into_iter(), client_id)
    }

    /// Whether any level restricts this user at all. A portal can use it to
    /// explain why a client is missing instead of showing an empty list.
    pub fn restricts(&self, user_id: &str, organizations: &[String], teams: &[String]) -> bool {
        self.users.contains_key(user_id)
            || organizations
                .iter()
                .any(|id| self.organizations.contains_key(id))
            || teams.iter().any(|id| self.teams.contains_key(id))
    }

    /// The clients this user may use, out of the ones the instance has
    /// registered. Disabled and soft-deleted clients are never offered.
    pub fn usable_clients<'a>(
        &self,
        user_id: &str,
        organizations: &[String],
        teams: &[String],
        clients: impl IntoIterator<Item = &'a oauth::client::Model>,
    ) -> Vec<String> {
        let mut out: Vec<String> = clients
            .into_iter()
            .filter(|client| client.enabled && client.deleted_at_ms.is_none())
            .filter(|client| self.allowed(user_id, organizations, teams, &client.id))
            .map(|client| client.id.clone())
            .collect();
        out.sort();
        out
    }
}

/// One level admits the client when it configures nothing, or when at least
/// one of its configured rows lists the client.
fn level<'a>(configured: impl Iterator<Item = &'a HashSet<String>>, client_id: &str) -> bool {
    let mut restricted = false;
    let mut admitted = false;
    for allowed in configured {
        restricted = true;
        admitted |= allowed.contains(client_id);
    }
    !restricted || admitted
}

/// Keep only the rows that configure a list. A JSON value that is not an array
/// counts as a configured-but-empty list, exactly as the SQL predicate sees it
/// (`allowlist IS NOT NULL` and no array element matches), so a malformed
/// value denies rather than quietly opening the level.
fn collect<'a>(
    rows: impl Iterator<Item = (&'a String, &'a Option<Value>)>,
) -> HashMap<String, HashSet<String>> {
    rows.filter_map(|(id, value)| {
        let value = value.as_ref()?;
        let allowed = value
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Some((id.clone(), allowed))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user_row(id: &str, allowlist: Option<Value>) -> user::Model {
        user::Model {
            id: id.into(),
            name: id.into(),
            password_hash: None,
            role: "user".into(),
            enabled: true,
            oauth_client_allowlist: allowlist,
            created_at_ms: 0,
        }
    }

    fn org_row(id: &str, allowlist: Option<Value>) -> organization::Model {
        organization::Model {
            id: id.into(),
            name: id.into(),
            oauth_client_allowlist: allowlist,
            created_at_ms: 0,
        }
    }

    fn team_row(id: &str, allowlist: Option<Value>) -> team::Model {
        team::Model {
            id: id.into(),
            organization_id: "acme".into(),
            name: id.into(),
            oauth_client_allowlist: allowlist,
            created_at_ms: 0,
        }
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn nothing_configured_anywhere_allows_everything() {
        let list = ClientAllowlist::build(
            &[user_row("alice", None)],
            &[org_row("acme", None)],
            &[team_row("core", None)],
        );
        assert!(list.allowed("alice", &ids(&["acme"]), &ids(&["core"]), "codex"));
        assert!(!list.restricts("alice", &ids(&["acme"]), &ids(&["core"])));
    }

    #[test]
    fn an_empty_list_denies_the_level_it_is_on() {
        let list = ClientAllowlist::build(&[user_row("alice", Some(json!([])))], &[], &[]);
        assert!(!list.allowed("alice", &[], &[], "codex"));
        assert!(list.restricts("alice", &[], &[]));
        // Another user is untouched by it.
        assert!(list.allowed("bob", &[], &[], "codex"));
    }

    #[test]
    fn a_configured_level_must_name_the_client() {
        let list = ClientAllowlist::build(
            &[user_row("alice", Some(json!(["codex", "claude"])))],
            &[],
            &[],
        );
        assert!(list.allowed("alice", &[], &[], "codex"));
        assert!(list.allowed("alice", &[], &[], "claude"));
        assert!(!list.allowed("alice", &[], &[], "gemini"));
    }

    #[test]
    fn levels_intersect_so_every_one_that_restricts_must_agree() {
        let list = ClientAllowlist::build(
            &[user_row("alice", Some(json!(["codex", "claude"])))],
            &[org_row("acme", Some(json!(["codex", "gemini"])))],
            &[team_row("core", Some(json!(["codex", "claude", "gemini"])))],
        );
        assert!(list.allowed("alice", &ids(&["acme"]), &ids(&["core"]), "codex"));
        // Allowed by the user and the team, not by the organization.
        assert!(!list.allowed("alice", &ids(&["acme"]), &ids(&["core"]), "claude"));
        // Allowed by the organization and the team, not by the user.
        assert!(!list.allowed("alice", &ids(&["acme"]), &ids(&["core"]), "gemini"));
    }

    #[test]
    fn within_one_level_the_configured_rows_are_a_union() {
        let list = ClientAllowlist::build(
            &[],
            &[
                org_row("acme", Some(json!(["codex"]))),
                org_row("globex", Some(json!(["claude"]))),
                org_row("initech", None),
            ],
            &[],
        );
        let orgs = ids(&["acme", "globex"]);
        assert!(list.allowed("alice", &orgs, &[], "codex"));
        assert!(list.allowed("alice", &orgs, &[], "claude"));
        assert!(!list.allowed("alice", &orgs, &[], "gemini"));
        // An unrestricted organization neither adds nor removes anything.
        let with_open = ids(&["acme", "initech"]);
        assert!(list.allowed("alice", &with_open, &[], "codex"));
        assert!(!list.allowed("alice", &with_open, &[], "claude"));
    }

    #[test]
    fn only_the_scopes_the_caller_is_in_are_considered() {
        let list = ClientAllowlist::build(&[], &[org_row("acme", Some(json!([])))], &[]);
        assert!(!list.allowed("alice", &ids(&["acme"]), &[], "codex"));
        assert!(list.allowed("alice", &ids(&["globex"]), &[], "codex"));
        assert!(list.allowed("alice", &[], &[], "codex"));
    }

    #[test]
    fn a_value_that_is_not_an_array_of_strings_restricts_rather_than_opens() {
        let list = ClientAllowlist::build(
            &[
                user_row("alice", Some(json!("codex"))),
                user_row("bob", Some(json!([1, 2, "codex"]))),
            ],
            &[],
            &[],
        );
        assert!(!list.allowed("alice", &[], &[], "codex"));
        assert!(list.allowed("bob", &[], &[], "codex"));
        assert!(!list.allowed("bob", &[], &[], "gemini"));
    }

    #[test]
    fn usable_clients_hides_disabled_and_deleted_registrations() {
        let client = |id: &str, enabled: bool, deleted: Option<i64>| oauth::client::Model {
            id: id.into(),
            name: id.into(),
            redirect_uris: json!([]),
            enabled,
            deleted_at_ms: deleted,
        };
        let list = ClientAllowlist::build(
            &[user_row("alice", Some(json!(["codex", "claude", "gone"])))],
            &[],
            &[],
        );
        let clients = vec![
            client("codex", true, None),
            client("claude", false, None),
            client("gone", true, Some(1)),
            client("gemini", true, None),
        ];
        assert_eq!(
            list.usable_clients("alice", &[], &[], &clients),
            vec!["codex".to_string()]
        );
    }
}
