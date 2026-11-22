//! MCP scope rules and tool nature overrides.
//!
//! Unlike `permissions`, a subject with no scope rule may use every MCP
//! server it has a credential for; a rule narrows that. A rule therefore
//! always has a subject, and a row that could not be evaluated is refused
//! here rather than dropped at assembly.

use gproxy_mcp::{scope::Scope as McpScope, tool::Nature};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Repository,
    entity::{identity::mcp_scope, upstream::mcp_tool_nature},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
    permissions::ACTIONS,
};
use crate::{
    AppError, Result,
    dto::{
        BatchItem, ListQuery, McpScopeDto, McpScopePatch, McpScopeWrite, McpToolNatureDto,
        McpToolNaturePatch, McpToolNatureWrite, Page,
    },
};

fn scope(text: &str) -> Result<String> {
    let text = crud::text(text, "scope")?;
    text.parse::<McpScope>()
        .map_err(|error| AppError::invalid(error.to_string()))?;
    Ok(text)
}

fn nature(text: &str) -> Result<String> {
    let names: Vec<&str> = Nature::ALL.iter().map(|nature| nature.as_str()).collect();
    crud::one_of(text, "nature", &names)
}

pub struct McpScopes<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> McpScopes<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> McpScopes<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<McpScopeDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<McpScopeDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: McpScopeWrite) -> Result<McpScopeDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: McpScopePatch) -> Result<McpScopeDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    /// A subject's rules are edited as a set; see `Permissions::batch`.
    pub async fn batch(
        &self,
        items: Vec<BatchItem<McpScopeWrite, McpScopePatch>>,
    ) -> Result<Vec<Option<McpScopeDto>>> {
        crud::batch(self, items).await
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for McpScopes<'_, C> {
    type Entity = mcp_scope::Entity;
    type Dto = McpScopeDto;
    type Write = McpScopeWrite;
    type Patch = McpScopePatch;

    const ENTITY: &'static str = "mcp scope";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().mcp_scopes()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Mcp]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = mcp_scope::Entity::find();
        if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
            select = select.filter(mcp_scope::Column::UserId.eq(user_id));
        }
        if let Some(owner) = crud::optional_text(query.owner_user_id.clone()) {
            select = select.filter(crud::owned_by(
                mcp_scope::Column::UserId,
                mcp_scope::Column::ApiKeyId,
                owner,
            ));
        }
        if let Some(api_key_id) = crud::optional_text(query.api_key_id.clone()) {
            select = select.filter(mcp_scope::Column::ApiKeyId.eq(api_key_id));
        }
        // A scope rule always has a subject, so none is a rule for everyone.
        if query.everyone == Some(true) {
            select = select
                .filter(mcp_scope::Column::UserId.is_null())
                .filter(mcp_scope::Column::ApiKeyId.is_null());
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(mcp_scope::Column::Scope.contains(&search));
        }
        select
    }

    async fn build(&self, write: McpScopeWrite) -> Result<(mcp_scope::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let user_id = crud::optional_text(write.user_id);
        let api_key_id = crud::optional_text(write.api_key_id);
        match (&user_id, &api_key_id) {
            (Some(user_id), None) => {
                crud::require_rows(
                    self.writer.store().users(),
                    "user",
                    std::slice::from_ref(user_id),
                )
                .await?;
            }
            (None, Some(api_key_id)) => {
                crud::require_rows(
                    self.writer.store().api_keys(),
                    "api key",
                    std::slice::from_ref(api_key_id),
                )
                .await?;
            }
            _ => {
                return Err(AppError::invalid(
                    "a scope rule names a user or an API key, not both and not neither",
                ));
            }
        }
        let row = mcp_scope::ActiveModel {
            id: Set(id.clone()),
            user_id: Set(user_id),
            api_key_id: Set(api_key_id),
            scope: Set(scope(&write.scope)?),
            action: Set(crud::one_of(&write.action, "action", &ACTIONS)?),
            priority: Set(write.priority.unwrap_or(0)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &mcp_scope::Model,
        patch: McpScopePatch,
    ) -> Result<mcp_scope::ActiveModel> {
        let mut row = mcp_scope::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(text) = patch.scope {
            row.scope = Set(scope(&text)?);
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

pub struct McpToolNatures<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> McpToolNatures<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> McpToolNatures<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<McpToolNatureDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<McpToolNatureDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: McpToolNatureWrite) -> Result<McpToolNatureDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: McpToolNaturePatch) -> Result<McpToolNatureDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<McpToolNatureWrite, McpToolNaturePatch>>,
    ) -> Result<Vec<Option<McpToolNatureDto>>> {
        crud::batch(self, items).await
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for McpToolNatures<'_, C> {
    type Entity = mcp_tool_nature::Entity;
    type Dto = McpToolNatureDto;
    type Write = McpToolNatureWrite;
    type Patch = McpToolNaturePatch;

    const ENTITY: &'static str = "mcp tool nature";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().mcp_tool_natures()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Mcp]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = mcp_tool_nature::Entity::find();
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(mcp_tool_nature::Column::ProviderId.eq(provider_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(mcp_tool_nature::Column::Tool.contains(&search));
        }
        select
    }

    async fn build(
        &self,
        write: McpToolNatureWrite,
    ) -> Result<(mcp_tool_nature::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let provider_id = crud::text(&write.provider_id, "providerId")?;
        crud::require_rows(
            self.writer.store().providers(),
            "provider",
            std::slice::from_ref(&provider_id),
        )
        .await?;
        let row = mcp_tool_nature::ActiveModel {
            id: Set(id.clone()),
            provider_id: Set(provider_id),
            tool: Set(crud::text(&write.tool, "tool")?),
            nature: Set(nature(&write.nature)?),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &mcp_tool_nature::Model,
        patch: McpToolNaturePatch,
    ) -> Result<mcp_tool_nature::ActiveModel> {
        let mut row = mcp_tool_nature::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(text) = patch.nature {
            row.nature = Set(nature(&text)?);
        }
        Ok(row)
    }
}
