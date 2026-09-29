use crate::{
    Result, Store, StoreError, entity::config::setting, error::invalid, repository::models,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, SelectProjection};
use sea_orm::Statement;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, Iterable, QueryTrait, Set};

pub struct Settings<'a, C> {
    db: &'a C,
}
impl<C: BatchConnectionTrait> Store<C> {
    pub fn settings(&self) -> Settings<'_, C> {
        Settings { db: &self.db }
    }
}
impl<C: BatchConnectionTrait> Settings<'_, C> {
    pub async fn get(&self) -> Result<Option<setting::Model>> {
        let query = setting::Entity::find_by_id(setting::GLOBAL_SETTINGS_ID)
            .batch_query(self.db.get_database_backend())?;
        let rows = self.db.query_batch(&[query]).await?;
        rows.into_iter()
            .next()
            .ok_or(StoreError::UnexpectedResult)?
            .first()
            .map(|r| sea_orm::FromQueryResult::from_query_result(r, "").map_err(Into::into))
            .transpose()
    }
    /// The upsert `update` would run, for callers that need the settings write
    /// inside their own revision batch (`Store::commit_revision`). Same
    /// validation and same SQL; the read-back is the caller's.
    pub fn update_statement(&self, mut patch: setting::ActiveModel) -> Result<Statement> {
        if patch
            .max_database_size_mb
            .try_as_ref()
            .is_some_and(|value| value.is_some_and(|v| v < 0))
        {
            return Err(invalid("database size must be nonnegative"));
        }
        if let Some(id) = patch.get_primary_key_value()
            && id != sea_orm::sea_query::ValueTuple::One(setting::GLOBAL_SETTINGS_ID.into())
        {
            return Err(invalid("global settings id must be 1"));
        }
        patch.id = Set(setting::GLOBAL_SETTINGS_ID);
        let columns = setting::Column::iter()
            .filter(|c| {
                !matches!(*c, setting::Column::Id) && matches!(patch.get(*c), ActiveValue::Set(_))
            })
            .collect::<Vec<_>>();
        let mut conflict = OnConflict::column(setting::Column::Id);
        if columns.is_empty() {
            conflict.do_nothing_on([setting::Column::Id]);
        } else {
            conflict.update_columns(columns);
        }
        Ok(setting::Entity::insert(patch)
            .on_conflict(conflict.to_owned())
            .build(self.db.get_database_backend()))
    }
    /// Upsert the singleton, changing only explicitly Set fields. No implicit
    /// database initialization is performed by Store::new.
    pub async fn update(&self, patch: setting::ActiveModel) -> Result<setting::Model> {
        let insert = self.update_statement(patch)?;
        let query = setting::Entity::find_by_id(setting::GLOBAL_SETTINGS_ID)
            .batch_query(self.db.get_database_backend())?;
        let result = self
            .db
            .batch(&[
                BatchStatement::Execute(insert),
                BatchStatement::Query(query),
            ])
            .await?;
        models::<setting::Model>(
            result
                .into_iter()
                .nth(1)
                .ok_or(StoreError::UnexpectedResult)?,
        )?
        .into_iter()
        .next()
        .ok_or(StoreError::UnexpectedResult)
    }
}
