//! Connection profiles: one complete outbound client configuration per row.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Repository, entity::config::connection_profile as profile};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};
use serde_json::Value;

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{
        BatchItem, ConnectionProfileDto, ConnectionProfilePatch, ConnectionProfileWrite, ListQuery,
        Page,
    },
};

const BACKENDS: [&str; 3] = ["reqwest", "wreq", "reqwest_native"];
const RETRIES: [&str; 2] = ["never", "default"];

pub struct ConnectionProfiles<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> ConnectionProfiles<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ConnectionProfiles<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<ConnectionProfileDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<ConnectionProfileDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: ConnectionProfileWrite) -> SdkResult<ConnectionProfileDto> {
        crud::create(self, write).await
    }
    pub async fn update(
        &self,
        id: &str,
        patch: ConnectionProfilePatch,
    ) -> SdkResult<ConnectionProfileDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<ConnectionProfileWrite, ConnectionProfilePatch>>,
    ) -> SdkResult<Vec<Option<ConnectionProfileDto>>> {
        crud::batch(self, items).await
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> SdkResult<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().connection_profiles(),
            Condition::all().add(profile::Column::Name.eq(&name)),
            exclude,
            || format!("a connection profile named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }
}

/// The emulation column is handed to `gproxy-client` as an object; anything
/// else would be dropped there without explanation.
fn emulation(value: Option<Value>) -> SdkResult<Option<Value>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) if value.is_object() => Ok(Some(value)),
        Some(_) => Err(SdkError::invalid("emulation must be a JSON object or null")),
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for ConnectionProfiles<'_, C> {
    type Entity = profile::Entity;
    type Dto = ConnectionProfileDto;
    type Write = ConnectionProfileWrite;
    type Patch = ConnectionProfilePatch;

    const ENTITY: &'static str = "connection profile";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().connection_profiles()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Profiles]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = profile::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(profile::Column::Name.contains(&search));
        }
        select
    }

    async fn build(
        &self,
        write: ConnectionProfileWrite,
    ) -> SdkResult<(profile::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = profile::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            backend: Set(crud::enumerated(
                write.backend.as_deref().unwrap_or("reqwest"),
                "backend",
                &BACKENDS,
            )?),
            emulation: Set(emulation(write.emulation)?),
            gzip: Set(write.gzip.unwrap_or(false)),
            brotli: Set(write.brotli.unwrap_or(false)),
            deflate: Set(write.deflate.unwrap_or(false)),
            zstd: Set(write.zstd.unwrap_or(false)),
            redirect_max_hops: Set(write.redirect_max_hops.unwrap_or(0)),
            retry: Set(crud::enumerated(
                write.retry.as_deref().unwrap_or("never"),
                "retry",
                &RETRIES,
            )?),
            connect_timeout_ms: Set(write.connect_timeout_ms.unwrap_or(10_000)),
            pool_idle_timeout_ms: Set(write.pool_idle_timeout_ms.unwrap_or(90_000)),
            pool_max_idle_per_host: Set(write.pool_max_idle_per_host.unwrap_or(32)),
            created_at_ms: Set(crate::rt::now_ms()),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &profile::Model,
        patch: ConnectionProfilePatch,
    ) -> SdkResult<profile::ActiveModel> {
        let mut row = profile::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(backend) = patch.backend {
            row.backend = Set(crud::enumerated(&backend, "backend", &BACKENDS)?);
        }
        if let Some(value) = patch.emulation {
            row.emulation = Set(emulation(value)?);
        }
        if let Some(value) = patch.gzip {
            row.gzip = Set(value);
        }
        if let Some(value) = patch.brotli {
            row.brotli = Set(value);
        }
        if let Some(value) = patch.deflate {
            row.deflate = Set(value);
        }
        if let Some(value) = patch.zstd {
            row.zstd = Set(value);
        }
        if let Some(value) = patch.redirect_max_hops {
            row.redirect_max_hops = Set(value);
        }
        if let Some(value) = patch.retry {
            row.retry = Set(crud::enumerated(&value, "retry", &RETRIES)?);
        }
        if let Some(value) = patch.connect_timeout_ms {
            row.connect_timeout_ms = Set(value);
        }
        if let Some(value) = patch.pool_idle_timeout_ms {
            row.pool_idle_timeout_ms = Set(value);
        }
        if let Some(value) = patch.pool_max_idle_per_host {
            row.pool_max_idle_per_host = Set(value);
        }
        Ok(row)
    }
}
