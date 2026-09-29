//! Resumable in-database upgrade. Old tables stay under `gproxy_v3_` until the
//! operator removes them. No traffic is served until the completion checkpoint.
mod guard;
use super::{Error, Result, source::Source};
use crate::{App, AppConfig};
use gproxy_sdk::{GproxyBuilder, SyncMode};
use gproxy_seaorm::{
    BatchConnectionTrait, BatchQuery, D1Type, Projection, SchemaSyncConnectionTrait,
};
use gproxy_store::{Store, StoreCache, entity::config::setting};
use guard::Guard;
use sea_orm::sea_query::{Alias, ColumnDef, Condition, Expr, ExprTrait, OnConflict, Query, Table};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult};
use std::sync::Arc;

pub const STATE: &str = "gproxy_v3_upgrade";
pub const PREFIX: &str = "gproxy_v3_";
// Checkpoints: 0 archive, 1 schema, 2 configuration, 3 identity, 4 usage, 5 complete.
const LEASE_MS: i64 = 60_000;
fn now() -> i64 {
    crate::now_ms()
}

/// `exclusive` is used only by a native host holding its database advisory
/// lock on a dedicated connection. Workers use the fenced, expiring lease.
pub async fn run<C>(db: C, config: &AppConfig, exclusive: bool) -> Result<bool>
where
    C: BatchConnectionTrait + SchemaSyncConnectionTrait + Clone + Send + Sync + 'static,
{
    let tables = db.table_names().await?;
    if !tables.iter().any(|t| t == STATE) || read_state(&db).await?.is_none() {
        if !tables.iter().any(|t| t == "schema_migrations") {
            return Ok(false);
        }
        if tables.iter().any(|t| t.starts_with(PREFIX) && t != STATE) {
            return Err(Error::other(
                "v3 migration archive names already exist without a migration checkpoint",
            ));
        }
        // Validate source and master key before changing any table names.
        let source = Source::new(&db, "").await?;
        let document = super::source::read(&source).await?;
        let bridge = bridge(config)?;
        super::check_the_key_matches_the_document(&document, &bridge)?;
        super::config::translate(&document, &bridge, now(), true)?;
        initialize(
            &db,
            &tables
                .into_iter()
                .filter(|t| t != STATE)
                .collect::<Vec<_>>(),
        )
        .await?;
    }
    let saved = state(&db).await?;
    if saved.try_get::<i32>("", "phase")? == 5 {
        return Ok(false);
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::other("cannot generate migration owner"))?;
    let owner = super::hex(&bytes);
    let mut claim = Query::update();
    claim
        .table(Alias::new(STATE))
        .value(Alias::new("owner"), owner.clone())
        .value(Alias::new("expires_at"), now() + LEASE_MS)
        .and_where(Expr::col(Alias::new("id")).eq(1))
        .and_where(Expr::col(Alias::new("phase")).lt(5));
    if !exclusive {
        claim.cond_where(
            Condition::any()
                .add(Expr::col(Alias::new("owner")).is_null())
                .add(Expr::col(Alias::new("expires_at")).lte(now())),
        );
    }
    if db
        .execute_raw(db.get_database_backend().build(&claim))
        .await?
        .rows_affected()
        != 1
    {
        if state(&db).await?.try_get::<i32>("", "phase")? == 5 {
            return Ok(false);
        }
        return Err(Error::other(
            "v3 migration is running in another instance; retry after its lease is released (at most 60 seconds after interruption)",
        ));
    }
    let guard = Guard {
        db: db.clone(),
        owner,
    };
    let result = migrate(&guard, config).await;
    let release = Query::update()
        .table(Alias::new(STATE))
        .value(Alias::new("owner"), Option::<String>::None)
        .value(Alias::new("expires_at"), 0i64)
        .and_where(Expr::col(Alias::new("id")).eq(1))
        .and_where(Expr::col(Alias::new("owner")).eq(&guard.owner))
        .to_owned();
    let released = db
        .execute_raw(db.get_database_backend().build(&release))
        .await;
    result?;
    released?;
    Ok(true)
}
fn bridge(config: &AppConfig) -> Result<super::secret::Bridge> {
    super::secret::Bridge::new(config.master_key.key.resolve()?)
}
async fn initialize<C: BatchConnectionTrait>(db: &C, tables: &[String]) -> Result<()> {
    let backend = db.get_database_backend();
    let create = Table::create()
        .table(Alias::new(STATE))
        .if_not_exists()
        .col(
            ColumnDef::new(Alias::new("id"))
                .integer()
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(Alias::new("phase"))
                .integer()
                .not_null()
                .default(0),
        )
        .col(ColumnDef::new(Alias::new("cursor")).big_integer())
        .col(ColumnDef::new(Alias::new("owner")).string_len(64))
        .col(
            ColumnDef::new(Alias::new("expires_at"))
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(ColumnDef::new(Alias::new("tables_json")).text())
        .col(ColumnDef::new(Alias::new("report_json")).json())
        .to_owned();
    db.execute_raw(backend.build(&create)).await?;
    let insert = Query::insert()
        .into_table(Alias::new(STATE))
        .columns([Alias::new("id"), Alias::new("tables_json")])
        .values_panic([
            1.into(),
            serde_json::to_string(tables)
                .map_err(|e| Error::other(e.to_string()))?
                .into(),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("id"))
                .do_nothing_on([Alias::new("id")])
                .to_owned(),
        )
        .to_owned();
    db.execute_raw(backend.build(&insert)).await?;
    Ok(())
}
async fn read_state<C: BatchConnectionTrait>(db: &C) -> Result<Option<QueryResult>> {
    let mut query = Query::select();
    query.columns([Alias::new("phase"), Alias::new("tables_json")]);
    let cursor = Expr::col(Alias::new("cursor"));
    query.expr_as(
        if db.requires_query_projection() {
            cursor.cast_as(Alias::new("TEXT"))
        } else {
            cursor
        },
        Alias::new("cursor"),
    );
    query
        .from(Alias::new(STATE))
        .and_where(Expr::col(Alias::new("id")).eq(1));
    let projection = Projection::new()
        .column("phase", D1Type::I32, false)?
        .column("cursor", D1Type::I64, true)?
        .column("tables_json", D1Type::Text, true)?;
    Ok(db
        .query_rows(BatchQuery::new(
            db.get_database_backend().build(&query),
            projection,
        ))
        .await?
        .into_iter()
        .next())
}
async fn state<C: BatchConnectionTrait>(db: &C) -> Result<QueryResult> {
    read_state(db)
        .await?
        .ok_or_else(|| Error::other("v3 migration checkpoint is missing"))
}
async fn checkpoint<C: BatchConnectionTrait>(
    db: &Guard<C>,
    phase: i32,
    cursor: Option<i64>,
) -> Result<()> {
    let cursor = if db.requires_query_projection() {
        Expr::val(cursor.map(|value| value.to_string())).cast_as(Alias::new("INTEGER"))
    } else {
        Expr::val(cursor)
    };
    let update = Query::update()
        .table(Alias::new(STATE))
        .value(Alias::new("phase"), phase)
        .value(Alias::new("cursor"), cursor)
        .and_where(Expr::col(Alias::new("id")).eq(1))
        .to_owned();
    db.execute_raw(db.get_database_backend().build(&update))
        .await?;
    Ok(())
}
async fn archive<C: BatchConnectionTrait + SchemaSyncConnectionTrait>(
    db: &Guard<C>,
    state: &QueryResult,
) -> Result<()> {
    let tables: Vec<String> = serde_json::from_str(&state.try_get::<String>("", "tables_json")?)
        .map_err(|e| Error::other(e.to_string()))?;
    let current = db.db.table_names().await?;
    let mut statements = Vec::new();
    for name in tables {
        let archived = format!("{PREFIX}{name}");
        if current.contains(&archived) && !current.contains(&name) {
            continue;
        }
        if current.contains(&archived) || !current.contains(&name) {
            return Err(Error::other(format!(
                "cannot archive v3 table {name}: unexpected source/archive state"
            )));
        }
        statements.push(
            db.get_database_backend()
                .build(Table::rename().table(Alias::new(name), Alias::new(archived))),
        );
    }
    if db.get_database_backend() == DbBackend::MySql {
        // MySQL DDL commits implicitly. One RENAME TABLE switches all names
        // atomically; the native host holds GET_LOCK across these commits.
        if !statements.is_empty() {
            let moves = statements
                .iter()
                .map(|s| s.sql.trim_start_matches("RENAME TABLE "))
                .collect::<Vec<_>>()
                .join(", ");
            db.db
                .execute_unprepared(&format!("RENAME TABLE {moves}"))
                .await?;
        }
    } else {
        db.atomic_batch(&statements).await?;
    }
    checkpoint(db, 1, None).await
}
async fn migrate<C>(db: &Guard<C>, config: &AppConfig) -> Result<()>
where
    C: BatchConnectionTrait + SchemaSyncConnectionTrait + Clone + Send + Sync + 'static,
{
    let mut saved = state(db).await?;
    if saved.try_get::<i32>("", "phase")? == 0 {
        archive(db, &saved).await?;
        saved = state(db).await?;
    }
    if saved.try_get::<i32>("", "phase")? == 1 {
        Store::new(db.db.clone()).resume_install().await?;
        checkpoint(db, 2, None).await?;
    }
    let store = Arc::new(Store::new(db.clone()));
    store
        .settings()
        .update(setting::ActiveModel::default())
        .await?;
    let mut builder = GproxyBuilder::connection(db.clone())
        .cache(Arc::new(StoreCache::new(store)))
        .sync_mode(SyncMode::Manual)
        .initial_reload(false);
    builder = match config.master_key.key.resolve()? {
        Some(key) => builder.master_key(key),
        None => builder.plaintext_secrets(),
    };
    let app = Arc::new(App::new(builder.build_unsynced().await?, config.clone()));
    let result = import(db, &app, config).await;
    app.gproxy().shutdown();
    result
}
async fn import<C>(db: &Guard<C>, app: &Arc<App<Guard<C>>>, config: &AppConfig) -> Result<()>
where
    C: BatchConnectionTrait + SchemaSyncConnectionTrait + Clone + Send + Sync + 'static,
{
    let source = Source::new(&db.db, PREFIX).await?;
    let document = super::source::read(&source).await?;
    let bridge = bridge(config)?;
    super::check_the_key_matches_the_document(&document, &bridge)?;
    let dropped = super::config::translate(&document, &bridge, now(), true)?.dropped_providers;
    let saved = state(db).await?;
    let mut phase: i32 = saved.try_get("", "phase")?;
    let mut cursor: Option<i64> = saved.try_get("", "cursor")?;
    if phase == 2 {
        super::refuse_a_populated_destination(app).await?;
        let (report, _) = super::import_configuration(app, &document, &bridge, true).await?;
        record_report(db, &report).await?;
        report.announce();
        checkpoint(db, 3, Some(0)).await?;
        phase = 3;
        cursor = Some(0);
    }
    app.reload_all().await?;
    if phase == 3 {
        loop {
            let (report, next) = super::identity::step(
                app,
                &document,
                &bridge,
                &dropped,
                cursor.unwrap_or(0) as usize,
            )
            .await?;
            record_report(db, &report).await?;
            report.announce();
            let Some(next) = next else {
                break;
            };
            cursor = Some(next as i64);
            checkpoint(db, 3, cursor).await?;
        }
        checkpoint(db, 4, None).await?;
        cursor = None;
    }
    if source.tables.iter().any(|t| t == "usage_rows") {
        loop {
            let rows = source
                .rows(
                    "usage_rows",
                    &["id"],
                    cursor.map(|id| ("id", id)),
                    Some(128),
                )
                .await?;
            if rows.is_empty() {
                break;
            }
            let mut records = Vec::new();
            let mut rounding = false;
            for row in rows {
                cursor = Some(row.try_get("", "id")?);
                let (record, rounded) = super::usage::translate(&row)?;
                if rounded {
                    rounding = true;
                }
                records.push(record);
            }
            let repo = app.gproxy().store().usage_records();
            let ids = records
                .iter()
                .map(|r| r.request_id.as_ref().clone())
                .collect::<Vec<_>>();
            let existing = repo
                .get_many(&ids)
                .await?
                .into_iter()
                .flatten()
                .map(|r| r.request_id)
                .collect::<std::collections::HashSet<_>>();
            records.retain(|r| !existing.contains(r.request_id.as_ref()));
            repo.insert_many(records).await?;
            if rounding {
                let mut report = super::Report::default();
                report.warn("historical costs rounded to v4 decimal precision; original values remain in metrics.v3.cost");
                record_report(db, &report).await?;
            }
            checkpoint(db, 4, cursor).await?;
        }
    }
    app.reload_all().await?;
    checkpoint(db, 5, None).await?;
    tracing::info!(
        archive_prefix = PREFIX,
        "v3 migration complete; original tables retained"
    );
    Ok(())
}

async fn record_report<C: BatchConnectionTrait>(
    db: &Guard<C>,
    report: &super::Report,
) -> Result<()> {
    if report.warnings.is_empty() && report.dropped.is_empty() {
        return Ok(());
    }
    let query = Query::select()
        .column(Alias::new("report_json"))
        .from(Alias::new(STATE))
        .and_where(Expr::col(Alias::new("id")).eq(1))
        .to_owned();
    let row = db
        .query_rows(BatchQuery::new(
            db.get_database_backend().build(&query),
            Projection::new().column("report_json", D1Type::Json, true)?,
        ))
        .await?
        .remove(0);
    let mut saved = row
        .try_get::<Option<serde_json::Value>>("", "report_json")?
        .unwrap_or_else(|| serde_json::json!({"warnings":[], "dropped":[]}));
    let warnings = saved["warnings"]
        .as_array_mut()
        .ok_or_else(|| Error::other("invalid migration report"))?;
    for warning in &report.warnings {
        let value = serde_json::json!(warning);
        if !warnings.contains(&value) {
            warnings.push(value);
        }
    }
    let dropped = saved["dropped"]
        .as_array_mut()
        .ok_or_else(|| Error::other("invalid migration report"))?;
    for row in &report.dropped {
        let value = serde_json::json!({"table":row.table,"row":row.row,"reason":row.reason});
        if !dropped.contains(&value) {
            dropped.push(value);
        }
    }
    let update = Query::update()
        .table(Alias::new(STATE))
        .value(Alias::new("report_json"), saved)
        .and_where(Expr::col(Alias::new("id")).eq(1))
        .to_owned();
    db.execute_raw(db.get_database_backend().build(&update))
        .await?;
    Ok(())
}
