//! Models exposed by a provider, optionally mapped to a global catalog entry.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "provider_models")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "upstream_model")]
    pub provider_id: String,
    #[sea_orm(unique_key = "upstream_model")]
    pub upstream_name: String,
    #[sea_orm(indexed)]
    pub model_id: Option<String>,
    pub metadata: Json,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<super::provider::Entity>,
    #[sea_orm(belongs_to, from = "model_id", to = "id", on_delete = "SetNull")]
    pub model: BelongsTo<Option<super::model::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Variant names share this row's upstream model; their behavior is stored
    /// in ordinary provider rewrite rules, never in this metadata.
    pub fn variant_names(&self) -> Vec<&str> {
        self.metadata
            .get("variants")
            .and_then(serde_json::Value::as_array)
            .map(|names| names.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default()
    }
    pub fn exposed_names(&self) -> Vec<&str> {
        if !self.enabled {
            return Vec::new();
        }
        let mut names = self.variant_names();
        if self
            .metadata
            .get("expose_base")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        {
            names.insert(0, &self.upstream_name);
        }
        names
    }
}
