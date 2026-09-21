//! `GET /admin/api/context`: what this caller may act as and act on.
//!
//! The console renders its navigation from this answer **and from nothing
//! else**. That is the design constraint the shape is chosen for: no other
//! request tells it which sections exist, so a section missing here is a
//! section the console does not draw, and a section present here is one the
//! surface will actually serve.
//!
//! It is the one route every authenticated caller reaches regardless of scope
//! — including a caller who administers several scopes and has not yet named
//! one, whose `scope` is `null` and whose `scopes` is the choice they have to
//! make.
//!
//! Nothing here is a secret. The organization and team names are ones the
//! caller administers, and the section list is a property of the release, not
//! of the data.

use serde::{Deserialize, Serialize};

use crate::{
    AdminAdmission, AdminScope, AppData, Caller, CallerKind,
    admin_scope::SCOPE_HEADER,
    admin_surface::{AdminSection, sections_for},
};

/// The whole answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct AdminContextDto {
    pub user: AdminContextUserDto,
    /// `apiKey`, `oauthGrant` or `session`. A console only ever sees
    /// `session`; a script reading its own context sees which credential it
    /// presented.
    pub caller_kind: String,
    /// The header a session names its scope with, so a console does not have
    /// to hard-code it.
    pub scope_header: String,
    /// What this request is acting as. `null` means the caller administers
    /// several scopes and named none — every other route answers `400` until
    /// one is named.
    pub scope: Option<AdminScopeDto>,
    /// Every scope this caller may act as. One entry for an instance
    /// administrator, one for an API key, one or more for a session.
    pub scopes: Vec<AdminScopeDto>,
    /// The sections `scope` reaches, in navigation order. Empty while `scope`
    /// is `null`.
    pub sections: Vec<AdminSectionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct AdminContextUserDto {
    pub id: String,
    /// The display name, when this revision's snapshot knows the user. A user
    /// created after the snapshot was taken is still a valid caller, so this
    /// is optional rather than a reason to fail.
    pub name: Option<String>,
    /// The instance-wide `users.role`. Organization and team roles are
    /// memberships and appear as `scopes`, not here.
    pub role: String,
    pub instance_admin: bool,
}

/// One scope the caller may act as.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct AdminScopeDto {
    /// `instance`, `organization` or `team`.
    pub kind: String,
    /// The organization or team id. `null` for the instance scope.
    pub id: Option<String>,
    /// Its display name, when the snapshot knows the row.
    pub name: Option<String>,
    /// For a team, its parent organization; for an organization, itself.
    pub organization_id: Option<String>,
    /// The exact value to send in the scope header to act as this.
    pub selector: String,
    pub current: bool,
}

/// One navigable family, as the console's navigation reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct AdminSectionDto {
    pub id: String,
    /// The collection path under `/admin/api`.
    pub path: String,
    /// `read`, `write`, or both.
    pub capabilities: Vec<String>,
}

impl AdminContextDto {
    /// Render one caller's context against the snapshot the request was
    /// decided under.
    pub fn build(caller: &Caller, admission: &AdminAdmission, data: &AppData) -> Self {
        let user = data.users.get(&caller.user_id);
        Self {
            user: AdminContextUserDto {
                id: caller.user_id.clone(),
                name: user.map(|row| row.name.clone()),
                role: caller.user_role.clone(),
                instance_admin: caller.is_instance_admin(),
            },
            caller_kind: match caller.kind {
                CallerKind::ApiKey => "apiKey",
                CallerKind::OAuthGrant => "oauthGrant",
                CallerKind::Session => "session",
            }
            .to_owned(),
            scope_header: SCOPE_HEADER.to_owned(),
            scope: admission
                .current
                .as_ref()
                .map(|scope| scope_dto(scope, data, true)),
            scopes: admission
                .available
                .iter()
                .map(|scope| scope_dto(scope, data, admission.current.as_ref() == Some(scope)))
                .collect(),
            sections: admission
                .current
                .as_ref()
                .map(|scope| sections_for(scope).map(section_dto).collect())
                .unwrap_or_default(),
        }
    }
}

fn scope_dto(scope: &AdminScope, data: &AppData, current: bool) -> AdminScopeDto {
    let (kind, id, name, organization_id) = match scope {
        AdminScope::Instance => ("instance", None, None, None),
        AdminScope::Organization(id) => (
            "organization",
            Some(id.clone()),
            data.organizations.get(id).map(|row| row.name.clone()),
            Some(id.clone()),
        ),
        AdminScope::Team(id) => {
            let team = data.teams.get(id);
            (
                "team",
                Some(id.clone()),
                team.map(|row| row.name.clone()),
                team.map(|row| row.organization_id.clone()),
            )
        }
    };
    AdminScopeDto {
        kind: kind.to_owned(),
        id,
        name,
        organization_id,
        selector: scope.selector(),
        current,
    }
}

fn section_dto(section: &'static AdminSection) -> AdminSectionDto {
    AdminSectionDto {
        id: section.id.to_owned(),
        path: section.path.to_owned(),
        capabilities: section
            .capabilities
            .iter()
            .map(|capability| (*capability).to_owned())
            .collect(),
    }
}
