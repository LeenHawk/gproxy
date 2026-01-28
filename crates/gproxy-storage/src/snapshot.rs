use sea_orm::{DbErr, EntityTrait, QueryOrder};

use crate::entities;
use crate::traffic::TrafficStorage;

pub type ProviderRow = entities::providers::Model;
pub type CredentialRow = entities::credentials::Model;
pub type DisallowRow = entities::credential_disallow::Model;
pub type UserRow = entities::users::Model;
pub type ApiKeyRow = entities::api_keys::Model;
pub type GlobalConfigRow = entities::global_config::Model;

#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    pub global_config: Option<GlobalConfigRow>,
    pub providers: Vec<ProviderRow>,
    pub credentials: Vec<CredentialRow>,
    pub disallow: Vec<DisallowRow>,
    pub users: Vec<UserRow>,
    pub api_keys: Vec<ApiKeyRow>,
}

impl TrafficStorage {
    pub async fn load_snapshot(&self) -> Result<StorageSnapshot, DbErr> {
        let global_config = entities::GlobalConfig::find()
            .order_by_asc(entities::global_config::Column::Id)
            .one(self.connection())
            .await?;
        let providers = entities::Providers::find().all(self.connection()).await?;
        let credentials = entities::Credentials::find().all(self.connection()).await?;
        let disallow = entities::CredentialDisallow::find()
            .all(self.connection())
            .await?;
        let users = entities::Users::find().all(self.connection()).await?;
        let api_keys = entities::ApiKeys::find().all(self.connection()).await?;

        Ok(StorageSnapshot {
            global_config,
            providers,
            credentials,
            disallow,
            users,
            api_keys,
        })
    }
}
