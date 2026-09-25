//! The shape every family has in common, and the six operations derived from
//! it.
//!
//! A family declares how a row is filtered, built and patched; list, get,
//! create, update, delete and batch follow from that. The point is not brevity
//! but uniformity: one place decides that a create reads its row back inside
//! the same transaction, that a missing id is `NotFound` rather than a silent
//! no-op, and that a batch is one revision commit however many rows it names.

use std::collections::HashSet;

use gproxy_seaorm::{BatchConnectionTrait, BatchResult, BatchStatement, SelectProjection};
use gproxy_store::{Repository, entity::upstream::provider};
use sea_orm::{ColumnTrait, EntityTrait, FromQueryResult, QueryFilter, Select};
use serde_json::Value;

use super::{Scope, Writer};
use crate::{
    SdkError, SdkResult,
    dto::{BatchItem, ListQuery, Page},
};

pub(crate) type EntityOf<S, C> = <S as Shape<C>>::Entity;
pub(crate) type ModelOf<S, C> = <EntityOf<S, C> as EntityTrait>::Model;
pub(crate) type ActiveOf<S, C> = <EntityOf<S, C> as EntityTrait>::ActiveModel;

/// One configuration family over one table with a `String` primary key.
pub(crate) trait Shape<C>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    type Entity: EntityTrait<PrimaryKey: sea_orm::PrimaryKeyTrait<ValueType = String>>;
    type Dto: From<ModelOf<Self, C>>;
    type Write;
    type Patch;

    /// What a `NotFound` calls this family's rows.
    const ENTITY: &'static str;

    fn writer(&self) -> Writer<'_, C>;
    fn repository(&self) -> Repository<'_, C, Self::Entity>;
    /// What a write of this family invalidates.
    fn scopes(&self) -> Vec<Scope>;
    /// What one specific update invalidates. Only credentials narrow this.
    fn update_scopes(&self, _id: &str, _patch: &Self::Patch) -> Vec<Scope> {
        self.scopes()
    }
    /// The filtered query a list pages over. Ordering is the repository's.
    fn select(&self, query: &ListQuery) -> Select<Self::Entity>;
    /// A validated new row with its primary key set, and that key.
    async fn build(&self, write: Self::Write) -> SdkResult<(ActiveOf<Self, C>, String)>;
    /// Extra owned rows can be created atomically with the parent, including batches.
    fn create_statements(&self, model: ActiveOf<Self, C>) -> SdkResult<Vec<BatchStatement>> {
        Ok(vec![BatchStatement::Execute(
            self.repository().insert_statement(model)?,
        )])
    }
    async fn delete_statements(&self, id: &str) -> SdkResult<Vec<BatchStatement>> {
        Ok(vec![BatchStatement::Execute(
            self.repository().delete_statement(id.to_owned()),
        )])
    }
    /// A validated patch of `current`, with the primary key set.
    async fn change(
        &self,
        current: &ModelOf<Self, C>,
        patch: Self::Patch,
    ) -> SdkResult<ActiveOf<Self, C>>;
}

pub(crate) async fn list<C, S>(shape: &S, query: ListQuery) -> SdkResult<Page<S::Dto>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    let (offset, limit) = query.bounds();
    let page = shape
        .repository()
        .page(shape.select(&query), offset, limit)
        .await?;
    Ok(Page::convert(page, S::Dto::from))
}

/// The row behind `id`, or `NotFound`. Every update and delete starts here, so
/// a caller never learns from a rows-affected count that its target was gone.
pub(crate) async fn row<C, S>(shape: &S, id: &str) -> SdkResult<ModelOf<S, C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    shape
        .repository()
        .get_many(&[id.to_owned()])
        .await?
        .into_iter()
        .next()
        .flatten()
        .ok_or_else(|| SdkError::not_found(S::ENTITY, id))
}

pub(crate) async fn get<C, S>(shape: &S, id: &str) -> SdkResult<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    Ok(S::Dto::from(row::<C, S>(shape, id).await?))
}

pub(crate) async fn create<C, S>(shape: &S, write: S::Write) -> SdkResult<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    let (model, id) = shape.build(write).await?;
    let statements = shape.create_statements(model)?;
    let scopes = shape.scopes();
    commit_one::<C, S>(shape, statements, &id, &scopes).await
}

pub(crate) async fn update<C, S>(shape: &S, id: &str, patch: S::Patch) -> SdkResult<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    let current = row::<C, S>(shape, id).await?;
    let scopes = shape.update_scopes(id, &patch);
    let model = shape.change(&current, patch).await?;
    // None means the patch set no column. The row still gets a revision, so a
    // caller that retries an already-applied patch sees the same answer as a
    // caller whose patch changed something.
    let statements = shape
        .repository()
        .update_statement(model)?
        .map(BatchStatement::Execute)
        .into_iter()
        .collect();
    commit_one::<C, S>(shape, statements, id, &scopes).await
}

pub(crate) async fn delete<C, S>(shape: &S, id: &str) -> SdkResult<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    row::<C, S>(shape, id).await?;
    let statements = shape.delete_statements(id).await?;
    shape.writer().commit(statements, &shape.scopes()).await?;
    Ok(())
}

/// Every item in one revision commit. A create's row is read back, an update's
/// too, and a delete has no row to return — hence `Option` per item, in the
/// order the items were given.
pub(crate) async fn batch<C, S>(
    shape: &S,
    items: Vec<BatchItem<S::Write, S::Patch>>,
) -> SdkResult<Vec<Option<S::Dto>>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let repository = shape.repository();
    let mut statements = Vec::new();
    let mut reads: Vec<Option<String>> = Vec::new();
    let scopes = shape.scopes();
    for item in items {
        match item {
            BatchItem::Create(write) => {
                let (model, id) = shape.build(write).await?;
                statements.extend(shape.create_statements(model)?);
                reads.push(Some(id));
            }
            BatchItem::Update(step) => {
                let current = row::<C, S>(shape, &step.id).await?;
                // A batch is one reload, so the narrowest scope of any item
                // cannot be honoured: keep the family's own.
                let model = shape.change(&current, step.patch).await?;
                if let Some(statement) = repository.update_statement(model)? {
                    statements.push(BatchStatement::Execute(statement));
                }
                reads.push(Some(step.id));
            }
            BatchItem::Delete(id) => {
                row::<C, S>(shape, &id).await?;
                statements.extend(shape.delete_statements(&id).await?);
                reads.push(None);
            }
        }
    }
    let writes = statements.len();
    let backend = shape.writer().backend();
    for id in reads.iter().flatten() {
        statements.push(BatchStatement::Query(
            S::Entity::find_by_id(id.clone()).batch_query(backend)?,
        ));
    }
    let (_, results) = shape.writer().commit_results(statements, &scopes).await?;
    let mut found = results.into_iter().skip(writes);
    reads
        .into_iter()
        .map(|id| match id {
            None => Ok(None),
            Some(id) => {
                let result = found
                    .next()
                    .ok_or(gproxy_store::StoreError::UnexpectedResult)?;
                let model = single::<ModelOf<S, C>>(result)?
                    .ok_or_else(|| SdkError::not_found(S::ENTITY, id))?;
                Ok(Some(S::Dto::from(model)))
            }
        })
        .collect()
}

/// Commit `statements` and read `id` back from the same transaction, so the
/// returned DTO is the row as the revision left it rather than a re-read that
/// a peer could already have changed.
async fn commit_one<C, S>(
    shape: &S,
    mut statements: Vec<BatchStatement>,
    id: &str,
    scopes: &[Scope],
) -> SdkResult<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    let backend = shape.writer().backend();
    statements.push(BatchStatement::Query(
        S::Entity::find_by_id(id.to_owned()).batch_query(backend)?,
    ));
    let (_, mut results) = shape.writer().commit_results(statements, scopes).await?;
    let result = results
        .pop()
        .ok_or(gproxy_store::StoreError::UnexpectedResult)?;
    let model = single::<ModelOf<S, C>>(result)?
        .ok_or_else(|| SdkError::not_found(S::ENTITY, id.to_owned()))?;
    Ok(S::Dto::from(model))
}

/// The first row of a batch result set, if there is one.
pub(crate) fn single<M: FromQueryResult>(result: BatchResult) -> SdkResult<Option<M>> {
    match result {
        BatchResult::Rows(rows) => rows
            .first()
            .map(|row| M::from_query_result(row, "").map_err(SdkError::Db))
            .transpose(),
        BatchResult::Executed(_) => {
            Err(SdkError::Store(gproxy_store::StoreError::UnexpectedResult))
        }
    }
}

/// Every row of a batch result set, for a family that reads back more than
/// one row inside its commit.
pub(crate) fn rows<M: FromQueryResult>(result: BatchResult) -> SdkResult<Vec<M>> {
    match result {
        BatchResult::Rows(rows) => rows
            .iter()
            .map(|row| M::from_query_result(row, "").map_err(SdkError::Db))
            .collect(),
        BatchResult::Executed(_) => {
            Err(SdkError::Store(gproxy_store::StoreError::UnexpectedResult))
        }
    }
}

// ---------------------------------------------------------------------------
// Validation shared by the families.
// ---------------------------------------------------------------------------

/// A caller-supplied id when it says something, a fresh one otherwise. A blank
/// string is treated as absent rather than as an id nothing can address.
pub(crate) fn id_or_new(supplied: Option<&str>) -> String {
    match supplied.map(str::trim) {
        Some(id) if !id.is_empty() => id.to_owned(),
        _ => crate::ids::random_id(),
    }
}

/// A required text column: trimmed, and never blank.
pub(crate) fn text(value: &str, field: &'static str) -> SdkResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SdkError::invalid(format!("{field} must not be blank")));
    }
    Ok(trimmed.to_owned())
}

/// An optional text column: blank collapses to None rather than to `""`.
pub(crate) fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// A JSON column that must hold an object. Null becomes an empty object, so a
/// caller that omits configuration gets the same row as one that sends `{}`.
pub(crate) fn object(value: Option<Value>, field: &'static str) -> SdkResult<Value> {
    match value {
        None | Some(Value::Null) => Ok(Value::Object(Default::default())),
        Some(value) if value.is_object() => Ok(value),
        Some(_) => Err(SdkError::invalid(format!("{field} must be a JSON object"))),
    }
}

/// An absolute URL. A relative one would be silently joined to whatever the
/// channel builds, which is never what an override meant.
pub(crate) fn url(value: &str, field: &'static str) -> SdkResult<String> {
    let trimmed = text(value, field)?;
    url::Url::parse(&trimmed)
        .map_err(|error| SdkError::invalid(format!("{field} is not a valid URL: {error}")))?;
    Ok(trimmed)
}

/// A decimal amount as it travels in a DTO.
pub(crate) fn decimal(value: &str, field: &'static str) -> SdkResult<gproxy_store::FixedDecimal> {
    value
        .trim()
        .parse()
        .map_err(|error| SdkError::invalid(format!("{field} is not a valid decimal: {error}")))
}

/// One of a fixed set of persisted enum spellings.
pub(crate) fn enumerated<E: sea_orm::ActiveEnum<Value = String>>(
    value: &str,
    field: &'static str,
    allowed: &[&str],
) -> SdkResult<E> {
    E::try_from_value(&value.trim().to_ascii_lowercase())
        .map_err(|_| SdkError::invalid(format!("{field} must be one of {}", allowed.join(", "))))
}

/// That every id in `ids` exists in `repository`, naming the first that does
/// not. Pre-checking keeps a foreign-key violation from reaching the caller as
/// an opaque database error.
pub(crate) async fn require_rows<C, E>(
    repository: Repository<'_, C, E>,
    entity: &'static str,
    ids: &[String],
) -> SdkResult<()>
where
    C: BatchConnectionTrait,
    E: EntityTrait<PrimaryKey: sea_orm::PrimaryKeyTrait<ValueType = String>>,
{
    if ids.is_empty() {
        return Ok(());
    }
    let found = repository.get_many(ids).await?;
    for (id, row) in ids.iter().zip(found) {
        if row.is_none() {
            return Err(SdkError::not_found(entity, id));
        }
    }
    Ok(())
}

/// The prefixes an exposed model name may not start with: a registered channel
/// id or an existing provider name. Both already mean something at resolution
/// time, and a public name that shadowed one could never be reached.
pub(crate) async fn reserved_prefixes<C>(writer: Writer<'_, C>) -> SdkResult<HashSet<String>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let mut reserved: HashSet<String> = writer
        .core()
        .channels()
        .ids()
        .map(str::to_ascii_lowercase)
        .collect();
    let providers = writer
        .store()
        .providers()
        .query(provider::Entity::find())
        .await?;
    reserved.extend(
        providers
            .into_iter()
            .map(|row| row.name.to_ascii_lowercase()),
    );
    Ok(reserved)
}

/// That no other row of the family already matches `condition`. `exclude` is
/// the row being updated, which is allowed to keep its own value.
pub(crate) async fn unique<C, E>(
    repository: Repository<'_, C, E>,
    condition: sea_orm::Condition,
    exclude: Option<&str>,
    message: impl FnOnce() -> String,
) -> SdkResult<()>
where
    C: BatchConnectionTrait,
    E: EntityTrait<PrimaryKey: sea_orm::PrimaryKeyTrait<ValueType = String>>,
{
    let mut query = E::find().filter(condition);
    if let Some(id) = exclude {
        query = query.filter(id_column::<E>().ne(id));
    }
    if !repository.query(query).await?.is_empty() {
        return Err(SdkError::conflict(message()));
    }
    Ok(())
}

fn id_column<E: EntityTrait>() -> E::Column {
    use sea_orm::{Iterable, PrimaryKeyToColumn};
    E::PrimaryKey::iter()
        .next()
        .expect("one primary key")
        .into_column()
}

pub(crate) fn proxy(value: Option<serde_json::Value>) -> SdkResult<Option<serde_json::Value>> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let mut proxy: gproxy_client::ProxyConfig = serde_json::from_value(value).map_err(|_| {
        crate::SdkError::invalid("proxy must be direct, system, or explicit with a URL")
    })?;
    if let gproxy_client::ProxyConfig::Explicit { url } = &mut proxy {
        let parsed = url::Url::parse(url.trim())
            .map_err(|_| crate::SdkError::invalid("invalid proxy URL"))?;
        if parsed.host_str().is_none() {
            return Err(crate::SdkError::invalid("proxy URL requires a host"));
        }
        *url = parsed[..url::Position::BeforePath].to_owned();
    }
    Ok(Some(
        serde_json::to_value(proxy).expect("proxy serialization"),
    ))
}
