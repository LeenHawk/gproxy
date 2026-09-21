//! Organizations and teams: the two scopes a gateway key can be bound to.
//!
//! # The explicit cascade, and why it is written out here
//!
//! `api_keys.organization_id` and `api_keys.team_id` declare
//! `on_delete = "Cascade"`, and on a database created from the current schema
//! the database enforces it. On an upgraded SQLite database it does not: those
//! two columns were added with `ALTER TABLE ADD COLUMN`, which cannot carry a
//! foreign key, so the constraint simply is not there. The other backends are
//! unverified.
//!
//! An instance therefore cannot rely on the cascade, and the rule it would
//! have enforced is not cosmetic. A key's organization or team decides three
//! things at once — the budget owner chain the call settles against, the
//! permission subject it is evaluated as, and the credential-visibility
//! boundary it may select from. A key whose binding points at a row that no
//! longer exists has none of the three, and on an upgraded instance nothing
//! below this layer would stop it from carrying on serving requests.
//!
//! So [`Organizations`] and [`Teams`] delete those keys themselves, in the
//! **same batch** as the parent row, through [`Shape::cascade`]. Deleting
//! rather than unbinding is deliberate: unbinding would silently widen a key
//! from "this team's credentials" to "everything its user can reach", which is
//! a privilege *increase* performed by a deletion. Because the explicit
//! statements run before the parent row is removed, a database that does have
//! the foreign key finds nothing left to cascade, and both database shapes end
//! up in exactly the same state.
//!
//! Deleting an organization also deletes its teams, and those teams' keys,
//! in that same batch: a team cannot exist without its organization, and the
//! `teams.organization_id` foreign key does exist on every database — but
//! relying on it would leave the team-bound keys behind on an upgraded one.

use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::{
    Repository,
    entity::identity::{api_key, organization, team},
};
use sea_orm::sea_query::Query;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    Result,
    dto::{
        BatchItem, ListQuery, OrganizationDto, OrganizationPatch, OrganizationWrite, Page, TeamDto,
        TeamPatch, TeamWrite,
    },
};

pub struct Organizations<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Organizations<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Organizations<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<OrganizationDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<OrganizationDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: OrganizationWrite) -> Result<OrganizationDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: OrganizationPatch) -> Result<OrganizationDto> {
        crud::update(self, id, patch).await
    }
    /// Delete the organization, its teams, and every API key bound to either,
    /// as one revision commit. See the module note.
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<OrganizationWrite, OrganizationPatch>>,
    ) -> Result<Vec<Option<OrganizationDto>>> {
        crud::batch(self, items).await
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> Result<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().organizations(),
            Condition::all().add(organization::Column::Name.eq(&name)),
            exclude,
            || format!("an organization named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Organizations<'_, C> {
    type Entity = organization::Entity;
    type Dto = OrganizationDto;
    type Write = OrganizationWrite;
    type Patch = OrganizationPatch;

    const ENTITY: &'static str = "organization";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().organizations()
    }
    fn scopes(&self) -> Vec<Scope> {
        // A deletion takes keys with it, so the key index has to be rebuilt
        // too; naming only `identity` would let a peer keep serving a key
        // whose scope is gone.
        vec![Scope::Identity, Scope::Keys]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = organization::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(organization::Column::Name.contains(&search));
        }
        select
    }

    async fn build(&self, write: OrganizationWrite) -> Result<(organization::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let row = organization::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            oauth_client_allowlist: Set(write
                .oauth_client_allowlist
                .map(|entries| crud::allowlist(entries, "oauthClientAllowlist"))
                .transpose()?),
            created_at_ms: Set(crate::now_ms()),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &organization::Model,
        patch: OrganizationPatch,
    ) -> Result<organization::ActiveModel> {
        let mut row = organization::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(allowlist) = patch.oauth_client_allowlist {
            row.oauth_client_allowlist = Set(allowlist
                .map(|entries| crud::allowlist(entries, "oauthClientAllowlist"))
                .transpose()?);
        }
        Ok(row)
    }

    /// The keys of this organization's teams, then the keys bound to the
    /// organization itself, then the teams. Order matters: the first statement
    /// reads `teams` in a subquery and must run while those rows are still
    /// there.
    async fn cascade(&self, id: &str) -> Result<Vec<BatchStatement>> {
        let keys = self.writer.store().api_keys();
        let teams_of = Query::select()
            .column(team::Column::Id)
            .from(team::Entity)
            .and_where(team::Column::OrganizationId.eq(id))
            .to_owned();
        Ok(vec![
            BatchStatement::Execute(
                keys.delete_where_statement(
                    api_key::Entity::delete_many()
                        .filter(api_key::Column::TeamId.in_subquery(teams_of)),
                ),
            ),
            BatchStatement::Execute(keys.delete_where_statement(
                api_key::Entity::delete_many().filter(api_key::Column::OrganizationId.eq(id)),
            )),
            BatchStatement::Execute(self.writer.store().teams().delete_where_statement(
                team::Entity::delete_many().filter(team::Column::OrganizationId.eq(id)),
            )),
        ])
    }
}

pub struct Teams<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Teams<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Teams<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<TeamDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<TeamDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: TeamWrite) -> Result<TeamDto> {
        crud::create(self, write).await
    }
    /// Patch a team. Its organization is deliberately not patchable: the
    /// team's credentials, budgets and bound keys were all scoped under the
    /// parent, and re-parenting the row would re-scope every one of them
    /// silently.
    pub async fn update(&self, id: &str, patch: TeamPatch) -> Result<TeamDto> {
        crud::update(self, id, patch).await
    }
    /// Delete the team and every API key bound to it, as one revision commit.
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<TeamWrite, TeamPatch>>,
    ) -> Result<Vec<Option<TeamDto>>> {
        crud::batch(self, items).await
    }

    async fn organization(&self, id: &str) -> Result<String> {
        let id = crud::text(id, "organizationId")?;
        crud::require_rows(
            self.writer.store().organizations(),
            "organization",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }

    /// Team names are unique within their organization, not globally: two
    /// organizations both having a `platform` team is normal.
    async fn name(
        &self,
        organization_id: &str,
        name: &str,
        exclude: Option<&str>,
    ) -> Result<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().teams(),
            Condition::all()
                .add(team::Column::OrganizationId.eq(organization_id))
                .add(team::Column::Name.eq(&name)),
            exclude,
            || format!("a team named `{name}` already exists in this organization"),
        )
        .await?;
        Ok(name)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Teams<'_, C> {
    type Entity = team::Entity;
    type Dto = TeamDto;
    type Write = TeamWrite;
    type Patch = TeamPatch;

    const ENTITY: &'static str = "team";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().teams()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Identity, Scope::Keys]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = team::Entity::find();
        if let Some(organization_id) = crud::optional_text(query.organization_id.clone()) {
            select = select.filter(team::Column::OrganizationId.eq(organization_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(team::Column::Name.contains(&search));
        }
        select
    }

    async fn build(&self, write: TeamWrite) -> Result<(team::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let organization_id = self.organization(&write.organization_id).await?;
        let row = team::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&organization_id, &write.name, None).await?),
            organization_id: Set(organization_id),
            oauth_client_allowlist: Set(write
                .oauth_client_allowlist
                .map(|entries| crud::allowlist(entries, "oauthClientAllowlist"))
                .transpose()?),
            created_at_ms: Set(crate::now_ms()),
        };
        Ok((row, id))
    }

    async fn change(&self, current: &team::Model, patch: TeamPatch) -> Result<team::ActiveModel> {
        let mut row = team::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self
                .name(&current.organization_id, &name, Some(&current.id))
                .await?);
        }
        if let Some(allowlist) = patch.oauth_client_allowlist {
            row.oauth_client_allowlist = Set(allowlist
                .map(|entries| crud::allowlist(entries, "oauthClientAllowlist"))
                .transpose()?);
        }
        Ok(row)
    }

    async fn cascade(&self, id: &str) -> Result<Vec<BatchStatement>> {
        Ok(vec![BatchStatement::Execute(
            self.writer.store().api_keys().delete_where_statement(
                api_key::Entity::delete_many().filter(api_key::Column::TeamId.eq(id)),
            ),
        )])
    }
}
