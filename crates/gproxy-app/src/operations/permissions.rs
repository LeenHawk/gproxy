//! Permission rules.
//!
//! A rule is a grant or a refusal for one subject over one provider (or all),
//! one model glob (or all) and one operation (or all). There is no implicit
//! grant: a caller with no applicable rule is refused, which is the whole
//! point of the table — adding a key must not silently add access to every
//! provider the instance has.
//!
//! Four things are checked, and each of them keeps a row out of the table that
//! could never mean what its author intended:
//!
//! - **exactly one subject.** Neither is a rule
//!   [`PermissionSet::build`](crate::snapshot::PermissionSet) drops on the
//!   floor; both is an intersection the snapshot honours for the sake of rows
//!   a v3 database may hold, but which nobody writing a rule means;
//! - **`action` is `allow` or `deny`.** Any other spelling is also dropped at
//!   assembly, so it would be a rule that silently does nothing;
//! - **the provider exists**, when one is named;
//! - **the model pattern is a usable glob** — blank matches nothing.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Repository, entity::identity::permission};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    Result,
    dto::{BatchItem, ListQuery, Page, PermissionDto, PermissionPatch, PermissionWrite},
};

/// The two values `permissions.action` takes.
pub const ACTIONS: [&str; 2] = ["allow", "deny"];

pub struct Permissions<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Permissions<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Permissions<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<PermissionDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<PermissionDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PermissionWrite) -> Result<PermissionDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PermissionPatch) -> Result<PermissionDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    /// Several rules in one revision commit.
    ///
    /// This is the operation a policy editor actually performs: a rule set is
    /// edited as a whole, and applying it row by row would leave the instance
    /// serving a half-applied policy — a deny removed before its replacement
    /// landed — for as many revisions as there were rows.
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PermissionWrite, PermissionPatch>>,
    ) -> Result<Vec<Option<PermissionDto>>> {
        crud::batch(self, items).await
    }

    async fn subject(
        &self,
        user_id: Option<String>,
        api_key_id: Option<String>,
    ) -> Result<(Option<String>, Option<String>)> {
        let user_id = crud::optional_text(user_id);
        let api_key_id = crud::optional_text(api_key_id);
        crud::one_subject(user_id.as_deref(), api_key_id.as_deref(), "permission rule")?;
        if let Some(user_id) = &user_id {
            crud::require_rows(
                self.writer.store().users(),
                "user",
                std::slice::from_ref(user_id),
            )
            .await?;
        }
        if let Some(api_key_id) = &api_key_id {
            crud::require_rows(
                self.writer.store().api_keys(),
                "api key",
                std::slice::from_ref(api_key_id),
            )
            .await?;
        }
        Ok((user_id, api_key_id))
    }

    async fn provider(&self, provider_id: Option<String>) -> Result<Option<String>> {
        let provider_id = crud::optional_text(provider_id);
        if let Some(provider_id) = &provider_id {
            crud::require_rows(
                self.writer.store().providers(),
                "provider",
                std::slice::from_ref(provider_id),
            )
            .await?;
        }
        Ok(provider_id)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Permissions<'_, C> {
    type Entity = permission::Entity;
    type Dto = PermissionDto;
    type Write = PermissionWrite;
    type Patch = PermissionPatch;

    const ENTITY: &'static str = "permission";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().permissions()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Permissions]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = permission::Entity::find();
        if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
            select = select.filter(permission::Column::UserId.eq(user_id));
        }
        if let Some(api_key_id) = crud::optional_text(query.api_key_id.clone()) {
            select = select.filter(permission::Column::ApiKeyId.eq(api_key_id));
        }
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(permission::Column::ProviderId.eq(provider_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(permission::Column::ModelPattern.contains(&search));
        }
        select
    }

    async fn build(&self, write: PermissionWrite) -> Result<(permission::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let (user_id, api_key_id) = self.subject(write.user_id, write.api_key_id).await?;
        let row = permission::ActiveModel {
            id: Set(id.clone()),
            user_id: Set(user_id),
            api_key_id: Set(api_key_id),
            provider_id: Set(self.provider(write.provider_id).await?),
            model_pattern: Set(crud::model_pattern(
                write.model_pattern.as_deref().unwrap_or("*"),
                "modelPattern",
            )?),
            operation: Set(crud::optional_text(write.operation)),
            action: Set(crud::one_of(&write.action, "action", &ACTIONS)?),
            priority: Set(write.priority.unwrap_or(0)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &permission::Model,
        patch: PermissionPatch,
    ) -> Result<permission::ActiveModel> {
        // The subject is re-validated as a pair, against the values the row
        // will actually hold: clearing one half and setting the other are two
        // patch fields but one rule.
        let user_id = patch.user_id.clone().unwrap_or(current.user_id.clone());
        let api_key_id = patch
            .api_key_id
            .clone()
            .unwrap_or(current.api_key_id.clone());
        let (user_id, api_key_id) = self.subject(user_id, api_key_id).await?;
        let mut row = permission::ActiveModel {
            id: Set(current.id.clone()),
            user_id: Set(user_id),
            api_key_id: Set(api_key_id),
            ..Default::default()
        };
        if let Some(provider_id) = patch.provider_id {
            row.provider_id = Set(self.provider(provider_id).await?);
        }
        if let Some(model_pattern) = patch.model_pattern {
            row.model_pattern = Set(crud::model_pattern(&model_pattern, "modelPattern")?);
        }
        if let Some(operation) = patch.operation {
            row.operation = Set(crud::optional_text(operation));
        }
        if let Some(action) = patch.action {
            row.action = Set(crud::one_of(&action, "action", &ACTIONS)?);
        }
        if let Some(priority) = patch.priority {
            row.priority = Set(priority);
        }
        Ok(row)
    }
}
