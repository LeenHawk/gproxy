//! Which organizations and teams a user belongs to, and with what role.
//!
//! Membership answers two different questions and they must not be confused.
//! *Use* of an organization's or team's shared credentials follows the calling
//! key's binding, not membership — see [`super::CredentialOwnership`].
//! Membership decides *management*: who may see and edit a shared credential,
//! a pool or a team, which is the `admin` role of that exact scope. A user's
//! instance-wide `users.role` is a third, separate thing.

use gproxy_store::entity::identity::{
    membership_role::MembershipRole, organization_member, team, team_member,
};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct MembershipIndex {
    organizations: HashMap<String, Vec<(String, MembershipRole)>>,
    teams: HashMap<String, Vec<(String, MembershipRole)>>,
    /// Team → its organization, so a team membership can be expanded to the
    /// parent organization the way the store's own policy queries do.
    parents: HashMap<String, String>,
}

impl MembershipIndex {
    pub fn build(
        organization_members: &[organization_member::Model],
        team_members: &[team_member::Model],
        teams: &[team::Model],
    ) -> Self {
        let mut organizations: HashMap<String, Vec<(String, MembershipRole)>> = HashMap::new();
        for row in organization_members {
            organizations
                .entry(row.user_id.clone())
                .or_default()
                .push((row.organization_id.clone(), row.role));
        }
        let mut team_index: HashMap<String, Vec<(String, MembershipRole)>> = HashMap::new();
        for row in team_members {
            team_index
                .entry(row.user_id.clone())
                .or_default()
                .push((row.team_id.clone(), row.role));
        }
        Self {
            organizations,
            teams: team_index,
            parents: teams
                .iter()
                .map(|team| (team.id.clone(), team.organization_id.clone()))
                .collect(),
        }
    }

    /// Direct organization memberships, in row order.
    pub fn organizations_of(&self, user_id: &str) -> &[(String, MembershipRole)] {
        self.organizations.get(user_id).map_or(&[], Vec::as_slice)
    }

    pub fn teams_of(&self, user_id: &str) -> &[(String, MembershipRole)] {
        self.teams.get(user_id).map_or(&[], Vec::as_slice)
    }

    /// Every organization the user is subject to: the ones joined directly and
    /// the parents of the teams joined. A user provisioned straight into a team
    /// has no `organization_members` row, and the store's OAuth policy query
    /// still holds them to that organization's rules, so the two must agree.
    pub fn effective_organizations(&self, user_id: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .organizations_of(user_id)
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        for (team_id, _) in self.teams_of(user_id) {
            if let Some(parent) = self.parents.get(team_id)
                && !out.contains(parent)
            {
                out.push(parent.clone());
            }
        }
        out
    }

    /// The organization a team belongs to.
    pub fn parent_of(&self, team_id: &str) -> Option<&str> {
        self.parents.get(team_id).map(String::as_str)
    }

    /// The role held in one organization, directly. Membership of a team does
    /// not confer an organization role, only exposure to its policies.
    pub fn role_in_org(&self, user_id: &str, organization_id: &str) -> Option<MembershipRole> {
        self.organizations_of(user_id)
            .iter()
            .find(|(id, _)| id == organization_id)
            .map(|(_, role)| *role)
    }

    pub fn role_in_team(&self, user_id: &str, team_id: &str) -> Option<MembershipRole> {
        self.teams_of(user_id)
            .iter()
            .find(|(id, _)| id == team_id)
            .map(|(_, role)| *role)
    }

    pub fn is_admin_of_org(&self, user_id: &str, organization_id: &str) -> bool {
        self.role_in_org(user_id, organization_id) == Some(MembershipRole::Admin)
    }

    /// An organization administrator is not automatically a team
    /// administrator: the roles are scoped to the row that grants them, and a
    /// caller that needs the wider rule must ask for both.
    pub fn is_admin_of_team(&self, user_id: &str, team_id: &str) -> bool {
        self.role_in_team(user_id, team_id) == Some(MembershipRole::Admin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org_member(org: &str, user: &str, role: MembershipRole) -> organization_member::Model {
        organization_member::Model {
            organization_id: org.into(),
            user_id: user.into(),
            role,
        }
    }

    fn team_member(team: &str, user: &str, role: MembershipRole) -> team_member::Model {
        team_member::Model {
            team_id: team.into(),
            user_id: user.into(),
            role,
        }
    }

    fn team_row(id: &str, org: &str) -> team::Model {
        team::Model {
            id: id.into(),
            organization_id: org.into(),
            name: id.into(),
            oauth_client_allowlist: None,
            created_at_ms: 0,
        }
    }

    fn index() -> MembershipIndex {
        MembershipIndex::build(
            &[
                org_member("acme", "admin", MembershipRole::Admin),
                org_member("acme", "member", MembershipRole::Member),
                org_member("other", "admin", MembershipRole::Member),
            ],
            &[
                team_member("core", "lead", MembershipRole::Admin),
                team_member("core", "member", MembershipRole::Member),
            ],
            &[team_row("core", "acme"), team_row("orphan", "gone")],
        )
    }

    #[test]
    fn roles_are_scoped_to_the_row_that_grants_them() {
        let index = index();
        assert_eq!(
            index.role_in_org("admin", "acme"),
            Some(MembershipRole::Admin)
        );
        assert_eq!(
            index.role_in_org("admin", "other"),
            Some(MembershipRole::Member)
        );
        assert_eq!(index.role_in_org("admin", "absent"), None);
        assert_eq!(index.role_in_org("stranger", "acme"), None);
        assert!(index.is_admin_of_org("admin", "acme"));
        assert!(!index.is_admin_of_org("admin", "other"));
        assert!(!index.is_admin_of_org("member", "acme"));
    }

    #[test]
    fn an_organization_admin_is_not_a_team_admin_by_itself() {
        let index = index();
        assert!(index.is_admin_of_org("admin", "acme"));
        assert!(!index.is_admin_of_team("admin", "core"));
        assert!(index.is_admin_of_team("lead", "core"));
        assert_eq!(
            index.role_in_team("member", "core"),
            Some(MembershipRole::Member)
        );
        assert_eq!(index.role_in_team("lead", "absent"), None);
    }

    #[test]
    fn a_team_membership_carries_its_parent_organization() {
        let index = index();
        assert_eq!(index.parent_of("core"), Some("acme"));
        assert_eq!(index.parent_of("absent"), None);
        // `lead` joined no organization directly, but belongs to acme's team.
        assert!(index.organizations_of("lead").is_empty());
        assert_eq!(index.effective_organizations("lead"), vec!["acme"]);
        // A direct membership and its team's parent are not listed twice.
        assert_eq!(index.effective_organizations("member"), vec!["acme"]);
        assert_eq!(
            index.effective_organizations("admin"),
            vec!["acme".to_string(), "other".to_string()]
        );
        assert!(index.effective_organizations("stranger").is_empty());
    }

    #[test]
    fn memberships_list_every_scope_joined() {
        let index = index();
        assert_eq!(index.organizations_of("admin").len(), 2);
        assert_eq!(index.teams_of("core").len(), 0);
        assert_eq!(index.teams_of("lead").len(), 1);
    }
}
