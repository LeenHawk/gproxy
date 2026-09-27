//! Thin typed SeaORM repositories. Business authorization/compilation belongs to core/manage.

use crate::{Result, StoreError, error::invalid};
use gproxy_seaorm::{
    BatchConnectionTrait, BatchQuery, BatchResult, BatchStatement, D1Type, Projection,
    SelectProjection,
};
use sea_orm::sea_query::{Alias, Expr, FromValueTuple, Func, IntoValueTuple, Query, ValueTuple};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, Condition, EntityTrait, FromQueryResult,
    IdenStatic, Iterable, ModelTrait, PrimaryKeyToColumn, PrimaryKeyTrait, QueryFilter,
    QuerySelect, QueryTrait, Select,
};
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    marker::PhantomData,
};

pub type Key<E> = <<E as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::ValueType;

pub struct Repository<'a, C, E> {
    pub(crate) db: &'a C,
    entity: PhantomData<E>,
}

impl<'a, C, E> Repository<'a, C, E> {
    pub(crate) fn new(db: &'a C) -> Self {
        Self {
            db,
            entity: PhantomData,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Page<M> {
    pub items: Vec<M>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

pub(crate) fn key_condition<E: EntityTrait>(key: ValueTuple) -> Condition {
    E::PrimaryKey::iter()
        .zip(key)
        .fold(Condition::all(), |condition, (column, value)| {
            condition.add(column.into_column().eq(value))
        })
}

pub(crate) fn active_key<E: EntityTrait>(model: &E::ActiveModel) -> Result<Key<E>> {
    let values = model
        .get_primary_key_value()
        .ok_or_else(|| invalid("all primary key fields must be supplied"))?;
    Ok(Key::<E>::from_value_tuple(values))
}

pub(crate) fn models<M: FromQueryResult>(result: BatchResult) -> Result<Vec<M>> {
    match result {
        BatchResult::Rows(rows) => rows
            .iter()
            .map(|row| M::from_query_result(row, "").map_err(Into::into))
            .collect(),
        _ => Err(StoreError::UnexpectedResult),
    }
}

pub(crate) fn affected(result: BatchResult) -> Result<u64> {
    match result {
        BatchResult::Executed(result) => Ok(result.rows_affected()),
        _ => Err(StoreError::UnexpectedResult),
    }
}

impl<C, E> Repository<'_, C, E>
where
    C: BatchConnectionTrait,
    E: EntityTrait,
    Key<E>: Clone + Eq + Hash + Sync,
{
    /// Conditions/joins/order remain SeaORM queries. Every set is decoded to its entity.
    pub async fn query_many(&self, queries: Vec<Select<E>>) -> Result<Vec<Vec<E::Model>>> {
        let prepared = queries
            .iter()
            .map(|q| q.batch_query(self.db.get_database_backend()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        self.db
            .query_batch(&prepared)
            .await?
            .into_iter()
            .map(|rows| {
                rows.iter()
                    .map(|r| E::Model::from_query_result(r, "").map_err(Into::into))
                    .collect()
            })
            .collect()
    }
    pub async fn query(&self, query: Select<E>) -> Result<Vec<E::Model>> {
        self.db
            .query_rows(query.batch_query(self.db.get_database_backend())?)
            .await?
            .iter()
            .map(|row| E::Model::from_query_result(row, "").map_err(Into::into))
            .collect()
    }
    /// Preserve input order, missing entries and duplicate requested IDs. Split
    /// IN-equivalent predicates at the connection's parameter cap, inside one batch.
    pub async fn get_many(&self, ids: &[Key<E>]) -> Result<Vec<Option<E::Model>>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut seen = HashSet::new();
        let unique = ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect::<Vec<_>>();
        let width = E::PrimaryKey::iter().count();
        let chunk = self.db.max_bind_parameters().unwrap_or(1000) / width.max(1);
        if chunk == 0 {
            return Err(invalid("primary key exceeds the parameter budget"));
        }
        let queries = unique
            .chunks(chunk)
            .map(|ids| {
                let condition = if width == 1 {
                    // IN avoids a linear OR expression tree, whose depth can
                    // exceed D1's limit even within its parameter budget.
                    let column = E::PrimaryKey::iter().next().expect("one primary key");
                    Condition::all().add(
                        column
                            .into_column()
                            .is_in(ids.iter().flat_map(|id| id.clone().into_value_tuple())),
                    )
                } else {
                    ids.iter().fold(Condition::any(), |c, id| {
                        c.add(key_condition::<E>(id.clone().into_value_tuple()))
                    })
                };
                E::find().filter(condition)
            })
            .collect();
        let mut found = HashMap::new();
        for row in self.query_many(queries).await?.into_iter().flatten() {
            found.insert(Key::<E>::from_value_tuple(row.get_primary_key_value()), row);
        }
        Ok(ids.iter().map(|id| found.get(id).cloned()).collect())
    }
    /// The write each `*_many` method would perform, for callers that compose
    /// their own batch (see `Store::commit_revision`). Same validation, same
    /// SQL; only the read-back and the batching are left to the caller.
    pub fn insert_statement(&self, item: E::ActiveModel) -> Result<sea_orm::Statement> {
        active_key::<E>(&item)?;
        Ok(E::insert(item).build(self.db.get_database_backend()))
    }
    /// None when the patch sets no non-key column: there is nothing to write,
    /// which is not an error.
    pub fn update_statement(&self, patch: E::ActiveModel) -> Result<Option<sea_orm::Statement>> {
        let key = active_key::<E>(&patch)?;
        let pk = E::PrimaryKey::iter()
            .map(|k| k.into_column().as_str())
            .collect::<Vec<_>>();
        let mut update = Query::update();
        update.table(E::default().table_ref());
        let mut changes = false;
        for column in E::Column::iter() {
            if !pk.contains(&column.as_str())
                && let ActiveValue::Set(value) = patch.get(column)
            {
                update.value(column, column.save_as(Expr::val(value)));
                changes = true;
            }
        }
        if !changes {
            return Ok(None);
        }
        update.cond_where(key_condition::<E>(key.into_value_tuple()));
        Ok(Some(self.db.get_database_backend().build(&update)))
    }
    pub fn delete_statement(&self, id: Key<E>) -> sea_orm::Statement {
        E::delete_by_id(id).build(self.db.get_database_backend())
    }
    pub fn update_where_statement(&self, query: sea_orm::UpdateMany<E>) -> sea_orm::Statement {
        query.build(self.db.get_database_backend())
    }
    pub fn delete_where_statement(&self, query: sea_orm::DeleteMany<E>) -> sea_orm::Statement {
        query.build(self.db.get_database_backend())
    }

    /// Insert `items` in one transaction without reading them back, for a
    /// caller that only needs them written. Half the statements of
    /// [`Self::create_many`].
    pub async fn insert_many(&self, items: Vec<E::ActiveModel>) -> Result<()> {
        let statements = items
            .into_iter()
            .map(|item| self.insert_statement(item))
            .collect::<Result<Vec<_>>>()?;
        if statements.is_empty() {
            return Ok(());
        }
        self.db.atomic_batch(&statements).await?;
        Ok(())
    }

    /// Caller-assigned keys; database defaults are read back in the same transaction.
    pub async fn create_many(&self, items: Vec<E::ActiveModel>) -> Result<Vec<E::Model>> {
        let keys = items
            .iter()
            .map(active_key::<E>)
            .collect::<Result<Vec<_>>>()?;
        let mut batch = items
            .into_iter()
            .map(|item| self.insert_statement(item).map(BatchStatement::Execute))
            .collect::<Result<Vec<_>>>()?;
        for id in keys {
            batch.push(BatchStatement::Query(
                E::find_by_id(id).batch_query(self.db.get_database_backend())?,
            ));
        }
        let writes = batch.len() / 2;
        self.db
            .batch(&batch)
            .await?
            .into_iter()
            .skip(writes)
            .map(|r| {
                models::<E::Model>(r)?
                    .into_iter()
                    .next()
                    .ok_or(StoreError::UnexpectedResult)
            })
            .collect()
    }
    /// Each patch has its own fields; NotSet/Unchanged fields are preserved.
    /// A missing ID returns None. Empty patches simply read the existing row.
    pub async fn update_many(&self, patches: Vec<E::ActiveModel>) -> Result<Vec<Option<E::Model>>> {
        let keys = patches
            .iter()
            .map(active_key::<E>)
            .collect::<Result<Vec<_>>>()?;
        let mut batch = Vec::new();
        for patch in patches {
            if let Some(statement) = self.update_statement(patch)? {
                batch.push(BatchStatement::Execute(statement));
            }
        }
        let writes = batch.len();
        for id in keys {
            batch.push(BatchStatement::Query(
                E::find_by_id(id).batch_query(self.db.get_database_backend())?,
            ));
        }
        self.db
            .batch(&batch)
            .await?
            .into_iter()
            .skip(writes)
            .map(|r| Ok(models::<E::Model>(r)?.into_iter().next()))
            .collect()
    }
    pub async fn delete_many(&self, ids: &[Key<E>]) -> Result<Vec<u64>> {
        let statements = ids
            .iter()
            .map(|id| self.delete_statement(id.clone()))
            .collect::<Vec<_>>();
        Ok(self
            .db
            .atomic_batch(&statements)
            .await?
            .into_iter()
            .map(|r| r.rows_affected())
            .collect())
    }

    /// Conditional bulk mutations are ordinary SeaORM builders, not a second
    /// filter language. Per-statement row counts retain the driver's semantics.
    pub async fn update_where_many(
        &self,
        queries: Vec<sea_orm::UpdateMany<E>>,
    ) -> Result<Vec<u64>> {
        let statements = queries
            .into_iter()
            .map(|q| self.update_where_statement(q))
            .collect::<Vec<_>>();
        Ok(self
            .db
            .atomic_batch(&statements)
            .await?
            .iter()
            .map(|r| r.rows_affected())
            .collect())
    }
    pub async fn delete_where_many(
        &self,
        queries: Vec<sea_orm::DeleteMany<E>>,
    ) -> Result<Vec<u64>> {
        let statements = queries
            .into_iter()
            .map(|q| self.delete_where_statement(q))
            .collect::<Vec<_>>();
        Ok(self
            .db
            .atomic_batch(&statements)
            .await?
            .iter()
            .map(|r| r.rows_affected())
            .collect())
    }
    /// Count the query's rows after filtering/grouping/distinct, before LIMIT/OFFSET.
    pub async fn count_many(&self, queries: Vec<Select<E>>) -> Result<Vec<u64>> {
        let prepared = queries
            .into_iter()
            .map(|q| count_query(q, self.db.get_database_backend()))
            .collect::<Result<Vec<_>>>()?;
        self.db
            .query_batch(&prepared)
            .await?
            .into_iter()
            .map(|rows| {
                let count: i64 = rows
                    .first()
                    .ok_or(StoreError::UnexpectedResult)?
                    .try_get("", "count")?;
                u64::try_from(count).map_err(|_| invalid("negative count"))
            })
            .collect()
    }
    /// Count and page share one database snapshot. Use an ordered parent query;
    /// load one-to-many relations separately rather than paging expanded JOIN rows.
    pub async fn page(
        &self,
        mut query: Select<E>,
        offset: u64,
        limit: u64,
    ) -> Result<Page<E::Model>> {
        if limit == 0 {
            return Err(invalid("page limit must be positive"));
        }
        let count = count_query(query.clone(), self.db.get_database_backend())?;
        if E::PrimaryKey::iter().count() > 0 {
            for key in E::PrimaryKey::iter() {
                sea_orm::QueryOrder::query(&mut query)
                    .order_by((E::default(), key.into_column()), sea_orm::Order::Asc);
            }
        }
        let page = query
            .limit(limit)
            .offset(offset)
            .batch_query(self.db.get_database_backend())?;
        let mut sets = self.db.query_batch(&[count, page]).await?.into_iter();
        let count = sets.next().ok_or(StoreError::UnexpectedResult)?;
        let total: i64 = count
            .first()
            .ok_or(StoreError::UnexpectedResult)?
            .try_get("", "count")?;
        let items = sets
            .next()
            .ok_or(StoreError::UnexpectedResult)?
            .iter()
            .map(|row| E::Model::from_query_result(row, "").map_err(Into::into))
            .collect::<Result<_>>()?;
        Ok(Page {
            items,
            total: u64::try_from(total).map_err(|_| invalid("negative count"))?,
            offset,
            limit,
        })
    }
}

fn count_query<E: EntityTrait>(
    query: Select<E>,
    backend: sea_orm::DbBackend,
) -> Result<BatchQuery> {
    let mut inner = query.into_query();
    inner.reset_limit().reset_offset().clear_order_by();
    let count = Query::select()
        .expr_as(
            Func::count(Expr::col(sea_orm::sea_query::Asterisk)),
            Alias::new("count"),
        )
        .from_subquery(inner, Alias::new("count_source"))
        .to_owned();
    Ok(BatchQuery::new(
        backend.build(&count),
        Projection::new().column("count", D1Type::I64, false)?,
    ))
}
