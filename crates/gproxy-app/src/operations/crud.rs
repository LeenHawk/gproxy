//! The shape every identity family has in common, and the operations derived
//! from it.
//!
//! A family declares how a row is filtered, built and patched; list, get,
//! create, update, delete and batch follow from that. The point is not brevity
//! but uniformity: one place decides that a create reads its row back inside
//! the same transaction, that a missing id is `NotFound` rather than a silent
//! no-op, and that a batch is one revision commit however many rows it names.
//!
//! This mirrors `gproxy-sdk`'s `manage::crud` deliberately, field for field,
//! so the two management surfaces behave identically from a console's point of
//! view. It is a copy rather than a shared crate because the two differ in the
//! two places that matter — the error type and what a commit does afterwards —
//! and a trait generic over both would have to name `AppError` and `SdkError`
//! in the same signature.

use gproxy_seaorm::{BatchConnectionTrait, BatchResult, BatchStatement, SelectProjection};
use gproxy_store::{Repository, StoreError};
use sea_orm::{ColumnTrait, EntityTrait, FromQueryResult, QueryFilter, Select};
use serde_json::Value;

use super::{Scope, Writer};
use crate::{
    AppError, Result,
    dto::{BatchItem, ListQuery, Page},
};

pub(crate) type EntityOf<S, C> = <S as Shape<C>>::Entity;
pub(crate) type ModelOf<S, C> = <EntityOf<S, C> as EntityTrait>::Model;
pub(crate) type ActiveOf<S, C> = <EntityOf<S, C> as EntityTrait>::ActiveModel;

/// One identity family over one table with a `String` primary key.
///
/// The composite-key tables — `organization_members` and `team_members` —
/// deliberately do not implement this: they have no surrogate id, so "get by
/// id", "patch by id" and "delete by id" have no meaning there and the member
/// family writes its own four operations instead.
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
    /// The filtered query a list pages over. Ordering is the repository's.
    fn select(&self, query: &ListQuery) -> Select<Self::Entity>;
    /// A validated new row with its primary key set, and that key.
    async fn build(&self, write: Self::Write) -> Result<(ActiveOf<Self, C>, String)>;
    /// A validated patch of `current`, with the primary key set.
    async fn change(
        &self,
        current: &ModelOf<Self, C>,
        patch: Self::Patch,
    ) -> Result<ActiveOf<Self, C>>;
    /// Statements that must run before the row itself is deleted.
    ///
    /// This exists for exactly one rule, and it is the reason it is on the
    /// trait rather than hidden in two families: **deleting an organization or
    /// a team has to remove the API keys bound to it explicitly**, because the
    /// foreign key that would cascade only exists in a database created from
    /// the current schema. See `operations::organizations`.
    async fn cascade(&self, _id: &str) -> Result<Vec<BatchStatement>> {
        Ok(Vec::new())
    }
}

pub(crate) async fn list<C, S>(shape: &S, query: ListQuery) -> Result<Page<S::Dto>>
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
pub(crate) async fn row<C, S>(shape: &S, id: &str) -> Result<ModelOf<S, C>>
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
        .ok_or_else(|| AppError::not_found(S::ENTITY, id))
}

pub(crate) async fn get<C, S>(shape: &S, id: &str) -> Result<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    Ok(S::Dto::from(row::<C, S>(shape, id).await?))
}

pub(crate) async fn create<C, S>(shape: &S, write: S::Write) -> Result<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    let (model, id) = shape.build(write).await?;
    let statement = shape.repository().insert_statement(model)?;
    let scopes = shape.scopes();
    commit_one::<C, S>(
        shape,
        vec![BatchStatement::Execute(statement)],
        &id,
        &scopes,
    )
    .await
}

pub(crate) async fn update<C, S>(shape: &S, id: &str, patch: S::Patch) -> Result<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    let current = row::<C, S>(shape, id).await?;
    let scopes = shape.scopes();
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

pub(crate) async fn delete<C, S>(shape: &S, id: &str) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    row::<C, S>(shape, id).await?;
    let mut statements = shape.cascade(id).await?;
    statements.push(BatchStatement::Execute(
        shape.repository().delete_statement(id.to_owned()),
    ));
    shape.writer().commit(statements, &shape.scopes()).await?;
    Ok(())
}

/// Every item in one revision commit. A create's row is read back, an update's
/// too, and a delete has no row to return — hence `Option` per item, in the
/// order the items were given.
pub(crate) async fn batch<C, S>(
    shape: &S,
    items: Vec<BatchItem<S::Write, S::Patch>>,
) -> Result<Vec<Option<S::Dto>>>
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
                statements.push(BatchStatement::Execute(repository.insert_statement(model)?));
                reads.push(Some(id));
            }
            BatchItem::Update(step) => {
                let current = row::<C, S>(shape, &step.id).await?;
                let model = shape.change(&current, step.patch).await?;
                if let Some(statement) = repository.update_statement(model)? {
                    statements.push(BatchStatement::Execute(statement));
                }
                reads.push(Some(step.id));
            }
            BatchItem::Delete(id) => {
                row::<C, S>(shape, &id).await?;
                statements.extend(shape.cascade(&id).await?);
                statements.push(BatchStatement::Execute(
                    repository.delete_statement(id.clone()),
                ));
                reads.push(None);
            }
        }
    }
    let writes = statements.len();
    let backend = shape.writer().backend();
    for id in reads.iter().flatten() {
        statements.push(BatchStatement::Query(
            S::Entity::find_by_id(id.clone())
                .batch_query(backend)
                .map_err(db)?,
        ));
    }
    let (_, results) = shape.writer().commit_results(statements, &scopes).await?;
    let mut found = results.into_iter().skip(writes);
    reads
        .into_iter()
        .map(|id| match id {
            None => Ok(None),
            Some(id) => {
                let result = found.next().ok_or(StoreError::UnexpectedResult)?;
                let model = single::<ModelOf<S, C>>(result)?
                    .ok_or_else(|| AppError::not_found(S::ENTITY, id))?;
                Ok(Some(S::Dto::from(model)))
            }
        })
        .collect()
}

/// Commit `statements` and read `id` back from the same transaction, so the
/// returned DTO is the row as the revision left it rather than a re-read a
/// peer could already have changed.
pub(crate) async fn commit_one<C, S>(
    shape: &S,
    mut statements: Vec<BatchStatement>,
    id: &str,
    scopes: &[Scope],
) -> Result<S::Dto>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
    S: Shape<C>,
{
    let backend = shape.writer().backend();
    statements.push(BatchStatement::Query(
        S::Entity::find_by_id(id.to_owned())
            .batch_query(backend)
            .map_err(db)?,
    ));
    let (_, mut results) = shape.writer().commit_results(statements, scopes).await?;
    let result = results.pop().ok_or(StoreError::UnexpectedResult)?;
    let model = single::<ModelOf<S, C>>(result)?
        .ok_or_else(|| AppError::not_found(S::ENTITY, id.to_owned()))?;
    Ok(S::Dto::from(model))
}

/// A driver error as this crate's error. `StoreError` is the only wrapper
/// `AppError` knows, and a raw `DbErr` reaching a caller would lose the
/// distinction between "the database said no" and "this layer said no".
fn db(error: sea_orm::DbErr) -> AppError {
    StoreError::from(error).into()
}

/// The first row of a batch result set, if there is one.
pub(crate) fn single<M: FromQueryResult>(result: BatchResult) -> Result<Option<M>> {
    match result {
        BatchResult::Rows(rows) => rows
            .first()
            .map(|row| M::from_query_result(row, "").map_err(db))
            .transpose(),
        BatchResult::Executed(_) => Err(StoreError::UnexpectedResult.into()),
    }
}

// ---------------------------------------------------------------------------
// Validation shared by the families.
// ---------------------------------------------------------------------------

/// An opaque row id. 16 bytes of entropy as lowercase hex, byte for byte what
/// core and the sdk mint.
///
/// Fallible, unlike the sdk's: a key or a session built on a failed fill would
/// be identical on every instance, and this crate already refuses to mint one
/// (`auth::api_key::random_bytes`). Ids are held to the same rule so there is
/// only one answer to "what happens when entropy fails".
pub(crate) fn random_id() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| AppError::internal("secure randomness is unavailable"))?;
    Ok(crate::hex::encode(&bytes))
}

/// A caller-supplied id when it says something, a fresh one otherwise. A blank
/// string is treated as absent rather than as an id nothing can address.
pub(crate) fn id_or_new(supplied: Option<&str>) -> Result<String> {
    match supplied.map(str::trim) {
        Some(id) if !id.is_empty() => Ok(id.to_owned()),
        _ => random_id(),
    }
}

/// A required text column: trimmed, and never blank.
pub(crate) fn text(value: &str, field: &'static str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid(format!("{field} must not be blank")));
    }
    Ok(trimmed.to_owned())
}

/// An optional text column: blank collapses to None rather than to `""`.
pub(crate) fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// One of a fixed set of lowercase spellings, as a plain string column. The
/// identity tables store `role`, `action` and `period` as text rather than as
/// a SeaORM enum, so this validates instead of parsing.
pub(crate) fn one_of(value: &str, field: &'static str, allowed: &[&str]) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if allowed.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(AppError::invalid(format!(
        "{field} must be one of {}",
        allowed.join(", ")
    )))
}

/// A decimal amount as it travels in a DTO.
pub(crate) fn decimal(value: &str, field: &'static str) -> Result<gproxy_store::FixedDecimal> {
    value
        .trim()
        .parse()
        .map_err(|error| AppError::invalid(format!("{field} is not a valid decimal: {error}")))
}

/// A model glob, as `snapshot::glob` and core's `rewrite::compile` read one.
///
/// Both accept every string — `*` and `?` are the only metacharacters and
/// neither can fail to compile — so the only way a pattern can be wrong is to
/// be blank, which would match nothing and is never what an operator meant.
/// Checking it here keeps a rule that can never fire out of the table.
pub(crate) fn model_pattern(value: &str, field: &'static str) -> Result<String> {
    let pattern = text(value, field)?;
    // Not a compilation failure, an intent failure: `**` and `*` are the same
    // glob, and a pattern that is only stars is `*` spelled at length.
    Ok(if pattern.chars().all(|ch| ch == '*') {
        "*".to_owned()
    } else {
        pattern
    })
}

/// A JSON allowlist column: an array of non-blank strings, deduplicated in the
/// order it was given.
pub(crate) fn allowlist(entries: Vec<String>, field: &'static str) -> Result<Value> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = text(&entry, field)?;
        if seen.insert(entry.clone()) {
            out.push(Value::String(entry));
        }
    }
    Ok(Value::Array(out))
}

/// That every id in `ids` exists in `repository`, naming the first that does
/// not. Pre-checking keeps a foreign-key violation from reaching the caller as
/// an opaque database error — and on the SQLite upgrade path, where the key
/// bindings carry no foreign key at all, it is the *only* check there is.
pub(crate) async fn require_rows<C, E>(
    repository: Repository<'_, C, E>,
    entity: &'static str,
    ids: &[String],
) -> Result<()>
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
            return Err(AppError::not_found(entity, id));
        }
    }
    Ok(())
}

/// That no other row of the family already matches `condition`. `exclude` is
/// the row being updated, which is allowed to keep its own value.
pub(crate) async fn unique<C, E>(
    repository: Repository<'_, C, E>,
    condition: sea_orm::Condition,
    exclude: Option<&str>,
    message: impl FnOnce() -> String,
) -> Result<()>
where
    C: BatchConnectionTrait,
    E: EntityTrait<PrimaryKey: sea_orm::PrimaryKeyTrait<ValueType = String>>,
{
    let mut query = E::find().filter(condition);
    if let Some(id) = exclude {
        query = query.filter(id_column::<E>().ne(id));
    }
    if !repository.query(query).await?.is_empty() {
        return Err(AppError::Conflict(message()));
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

/// Exactly one of a rule's two subject columns, which is what makes a rule
/// addressable.
///
/// Neither is a grant to nobody — or, read the other way round by a future
/// evaluator, a grant to everybody — and `PermissionSet::build` already drops
/// such a row on the floor, so writing one produces a row that silently does
/// nothing. Both is an intersection: the snapshot honours it, because a v3
/// database may hold one, but nobody who writes a rule for a key *and* a user
/// means "only when this key is used by this user", so it is refused at the
/// door rather than stored and misread later.
pub(crate) fn one_subject(
    user_id: Option<&str>,
    api_key_id: Option<&str>,
    what: &'static str,
) -> Result<()> {
    match (user_id, api_key_id) {
        (Some(_), None) | (None, Some(_)) => Ok(()),
        (None, None) => Err(AppError::invalid(format!(
            "a {what} needs a userId or an apiKeyId"
        ))),
        (Some(_), Some(_)) => Err(AppError::invalid(format!(
            "a {what} takes a userId or an apiKeyId, not both"
        ))),
    }
}

/// A range whose ends are both known must not be inverted. An open end is not
/// a range error: a subscription with no expiry is the normal case.
pub(crate) fn ordered_range(start: Option<i64>, end: Option<i64>) -> Result<()> {
    if let (Some(start), Some(end)) = (start, end)
        && start > end
    {
        return Err(AppError::invalid(
            "startsAtMs must not be after expiresAtMs",
        ));
    }
    Ok(())
}
