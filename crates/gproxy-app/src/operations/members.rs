//! Organization and team membership.
//!
//! Both tables are composite-keyed — `(organization_id, user_id)` and
//! `(team_id, user_id)` — with no surrogate id, so this family does not go
//! through [`Shape`](super::crud::Shape): there is no "get by id" and no
//! "patch by id" to derive. The three operations a membership actually has
//! are written out instead: add, change the role, remove.
//!
//! A membership grants the *use* of that scope's shared credentials. The
//! `admin` role additionally grants managing them, which is why changing a
//! role is a separate operation from adding a member and not a field of it.

use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::identity::{
    membership_role::MembershipRole, organization_member, team_member,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};

use super::{Scope, Writer, crud};
use crate::{
    AppError, Result,
    dto::{ListQuery, MemberPatch, MemberWrite, OrganizationMemberDto, Page, TeamMemberDto},
};

/// The two values a membership role takes.
pub const MEMBERSHIP_ROLES: [&str; 2] = ["member", "admin"];

fn role(value: Option<&str>) -> Result<MembershipRole> {
    match crud::one_of(value.unwrap_or("member"), "role", &MEMBERSHIP_ROLES)?.as_str() {
        "admin" => Ok(MembershipRole::Admin),
        _ => Ok(MembershipRole::Member),
    }
}

pub struct OrganizationMembers<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> OrganizationMembers<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> OrganizationMembers<'_, C> {
    /// Everyone in one organization, or everywhere one user is a member, or
    /// both. A query that names neither pages the whole table, which is what
    /// an instance-wide membership report is.
    pub async fn list(&self, query: ListQuery) -> Result<Page<OrganizationMemberDto>> {
        let (offset, limit) = query.bounds();
        let page = self
            .writer
            .store()
            .organization_members()
            .page(select(&query), offset, limit)
            .await?;
        Ok(Page::convert(page, OrganizationMemberDto::from))
    }

    pub async fn get(&self, organization_id: &str, user_id: &str) -> Result<OrganizationMemberDto> {
        Ok(OrganizationMemberDto::from(
            self.row(organization_id, user_id).await?,
        ))
    }

    /// Add a member. A repeat is a `Conflict` rather than a silent no-op: the
    /// caller asked to add somebody who is already in, and the role they
    /// intended may differ from the one that is stored.
    pub async fn add(
        &self,
        organization_id: &str,
        write: MemberWrite,
    ) -> Result<OrganizationMemberDto> {
        let organization_id = crud::text(organization_id, "organizationId")?;
        let user_id = crud::text(&write.user_id, "userId")?;
        crud::require_rows(
            self.writer.store().organizations(),
            "organization",
            std::slice::from_ref(&organization_id),
        )
        .await?;
        crud::require_rows(
            self.writer.store().users(),
            "user",
            std::slice::from_ref(&user_id),
        )
        .await?;
        if self.find(&organization_id, &user_id).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "user `{user_id}` is already a member of organization `{organization_id}`"
            )));
        }
        let row = organization_member::ActiveModel {
            organization_id: Set(organization_id.clone()),
            user_id: Set(user_id.clone()),
            role: Set(role(write.role.as_deref())?),
        };
        let statement = self
            .writer
            .store()
            .organization_members()
            .insert_statement(row)?;
        self.writer
            .commit(vec![BatchStatement::Execute(statement)], &[Scope::Identity])
            .await?;
        self.get(&organization_id, &user_id).await
    }

    pub async fn set_role(
        &self,
        organization_id: &str,
        user_id: &str,
        patch: MemberPatch,
    ) -> Result<OrganizationMemberDto> {
        self.row(organization_id, user_id).await?;
        let row = organization_member::ActiveModel {
            organization_id: Set(organization_id.to_owned()),
            user_id: Set(user_id.to_owned()),
            role: Set(role(Some(&patch.role))?),
        };
        let statement = self
            .writer
            .store()
            .organization_members()
            .update_statement(row)?
            .ok_or_else(|| AppError::internal("role patch set no column"))?;
        self.writer
            .commit(vec![BatchStatement::Execute(statement)], &[Scope::Identity])
            .await?;
        self.get(organization_id, user_id).await
    }

    /// Remove a member.
    ///
    /// Keys bound to the organization are deliberately **not** touched: a key
    /// binding is set by an operator, and a membership change is not the place
    /// to destroy one silently. Admission re-reads the binding on every
    /// request, and credential visibility answers from the key's binding
    /// rather than from the holder's memberships, so a removed member's key
    /// keeps exactly the reach its row states until an operator changes it.
    pub async fn remove(&self, organization_id: &str, user_id: &str) -> Result<()> {
        self.row(organization_id, user_id).await?;
        let statement = self
            .writer
            .store()
            .organization_members()
            .delete_statement((organization_id.to_owned(), user_id.to_owned()));
        self.writer
            .commit(vec![BatchStatement::Execute(statement)], &[Scope::Identity])
            .await?;
        Ok(())
    }

    async fn find(
        &self,
        organization_id: &str,
        user_id: &str,
    ) -> Result<Option<organization_member::Model>> {
        Ok(self
            .writer
            .store()
            .organization_members()
            .get_many(&[(organization_id.to_owned(), user_id.to_owned())])
            .await?
            .into_iter()
            .next()
            .flatten())
    }

    async fn row(
        &self,
        organization_id: &str,
        user_id: &str,
    ) -> Result<organization_member::Model> {
        self.find(organization_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("organization member", user_id))
    }
}

fn select(query: &ListQuery) -> Select<organization_member::Entity> {
    let mut select = organization_member::Entity::find();
    if let Some(organization_id) = crud::optional_text(query.organization_id.clone()) {
        select = select.filter(organization_member::Column::OrganizationId.eq(organization_id));
    }
    if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
        select = select.filter(organization_member::Column::UserId.eq(user_id));
    }
    select
}

pub struct TeamMembers<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> TeamMembers<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> TeamMembers<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<TeamMemberDto>> {
        let (offset, limit) = query.bounds();
        let mut select = team_member::Entity::find();
        if let Some(team_id) = crud::optional_text(query.team_id.clone()) {
            select = select.filter(team_member::Column::TeamId.eq(team_id));
        }
        if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
            select = select.filter(team_member::Column::UserId.eq(user_id));
        }
        let page = self
            .writer
            .store()
            .team_members()
            .page(select, offset, limit)
            .await?;
        Ok(Page::convert(page, TeamMemberDto::from))
    }

    pub async fn get(&self, team_id: &str, user_id: &str) -> Result<TeamMemberDto> {
        Ok(TeamMemberDto::from(self.row(team_id, user_id).await?))
    }

    /// Add a member to a team.
    ///
    /// Membership of the parent organization is **not** required. A team is
    /// its own credential-visibility scope, and an instance that wants the
    /// stricter rule adds both memberships; requiring it here would make
    /// "contractor on one team" impossible to express.
    pub async fn add(&self, team_id: &str, write: MemberWrite) -> Result<TeamMemberDto> {
        let team_id = crud::text(team_id, "teamId")?;
        let user_id = crud::text(&write.user_id, "userId")?;
        crud::require_rows(
            self.writer.store().teams(),
            "team",
            std::slice::from_ref(&team_id),
        )
        .await?;
        crud::require_rows(
            self.writer.store().users(),
            "user",
            std::slice::from_ref(&user_id),
        )
        .await?;
        if self.find(&team_id, &user_id).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "user `{user_id}` is already a member of team `{team_id}`"
            )));
        }
        let statement =
            self.writer
                .store()
                .team_members()
                .insert_statement(team_member::ActiveModel {
                    team_id: Set(team_id.clone()),
                    user_id: Set(user_id.clone()),
                    role: Set(role(write.role.as_deref())?),
                })?;
        self.writer
            .commit(vec![BatchStatement::Execute(statement)], &[Scope::Identity])
            .await?;
        self.get(&team_id, &user_id).await
    }

    pub async fn set_role(
        &self,
        team_id: &str,
        user_id: &str,
        patch: MemberPatch,
    ) -> Result<TeamMemberDto> {
        self.row(team_id, user_id).await?;
        let statement = self
            .writer
            .store()
            .team_members()
            .update_statement(team_member::ActiveModel {
                team_id: Set(team_id.to_owned()),
                user_id: Set(user_id.to_owned()),
                role: Set(role(Some(&patch.role))?),
            })?
            .ok_or_else(|| AppError::internal("role patch set no column"))?;
        self.writer
            .commit(vec![BatchStatement::Execute(statement)], &[Scope::Identity])
            .await?;
        self.get(team_id, user_id).await
    }

    pub async fn remove(&self, team_id: &str, user_id: &str) -> Result<()> {
        self.row(team_id, user_id).await?;
        let statement = self
            .writer
            .store()
            .team_members()
            .delete_statement((team_id.to_owned(), user_id.to_owned()));
        self.writer
            .commit(vec![BatchStatement::Execute(statement)], &[Scope::Identity])
            .await?;
        Ok(())
    }

    async fn find(&self, team_id: &str, user_id: &str) -> Result<Option<team_member::Model>> {
        Ok(self
            .writer
            .store()
            .team_members()
            .get_many(&[(team_id.to_owned(), user_id.to_owned())])
            .await?
            .into_iter()
            .next()
            .flatten())
    }

    async fn row(&self, team_id: &str, user_id: &str) -> Result<team_member::Model> {
        self.find(team_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("team member", user_id))
    }
}
