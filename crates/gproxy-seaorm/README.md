# gproxy-seaorm

English | [简体中文](README.zh-CN.md)

Standalone SeaORM 2 batch operations and a libSQL/Turso adapter. Application
entities stay in your application. This crate provides parameter and result
conversion for projected connections, portable atomic batches, entity-first
schema synchronization, and integration with the official `sea-orm-migration`
crate. It does not depend on gproxy core.

## Entity queries

With the `libsql` feature, `LibsqlConnection` implements SeaORM's
`ConnectionTrait` over a remote database whose rows arrive untyped. Select an
entity or an explicit result projection for the operation:

```rust
use gproxy_seaorm::LibsqlConnection;
use sea_orm::EntityTrait;

let connection = LibsqlConnection::new(transport, "libsql://db.turso.io", Some(token))?;
let providers = connection.for_entity::<provider::Entity>()?;
let rows = provider::Entity::find().all(&providers).await?;
```

`for_entity` derives types and nullability from `Column::def()`. Each connection
view owns its projection; tables do not share a mutable column-name registry.
When overriding entity SQL column types or using custom Rust value types, check
that the column metadata matches the Rust decoding type. Use an explicit
projection when they differ.

## Automatic entity and relation projections

Import `SelectProjection` to derive the SQL result projection from a normal
SeaORM select. This covers a single entity, two-entity optional/required/grouped
joins, and three-to-six-entity selects. One-to-many and many-to-many relations
continue to use SeaORM's normal relation definitions and grouping.

```rust
use gproxy_seaorm::{BatchConnectionTrait, SelectProjection};
use sea_orm::{DbBackend, EntityTrait};

let query = provider::Entity::find().find_with_related(credential::Entity);

// Single query: retain SeaORM's (provider, Vec<credential>) result.
let view = connection.for_select(&query)?;
let models = query.clone().all(&view).await?;

// Portable batch: return the ordered flat row set for each prepared query.
let prepared = query.batch_query(DbBackend::Sqlite)?;
let result_sets = connection.query_batch(&[prepared]).await?;
```

The helpers infer SeaORM's `A_`, `B_`, `C_`, etc. aliases and mark optional joined
entities nullable, including their primary keys. Required INNER JOIN entities
retain their declared nullability. Renamed SQL columns and standard ActiveEnum
selects are supported. Filters, ordering, LIMIT/OFFSET and selecting a subset of
normal columns do not require manual mappings; unselected unsupported types do
not prevent deriving a supported subset.

WHERE conditions are preserved as parameterized SeaORM SQL, including AND/OR,
IN, LIKE, comparisons, NULL checks, related-column filters and EXISTS subqueries.
Explicit projection requirements concern SELECT outputs, not WHERE expressions.

Construct the final selection first, then derive its view/batch query. Grouped
three/four-table wrappers do not expose SeaORM QueryTrait: derive from the normal
select before converting it to the grouped wrapper. A paginator's count query is
a separate aggregate and needs its own explicit projection.

Computed expressions, custom aliases/table aliases (including linked/self-join
queries that introduce them), undeclared select casts, duplicate result aliases and
unsupported selected column types are rejected while preparing the automatic
projection, before dispatch. Use `BatchQuery::new(statement, projection)` or
`with_projection` for these queries. Automatic generation does not parse arbitrary
SQL or infer aggregates from output names. Entity-declared `select_as` expressions
are recognized by exact expression matching and retain the column metadata.

For manual joins using whole entity column sets, compose
`Projection::for_entity_prefixed::<E>(prefix, nullable)` with `.merge(...)`.
The nullable argument applies to the entire optional entity. Decoding checks
remain in force; projections are not silently guessed when missing.

## Custom queries and result order

Use `Projection::column(alias, type, nullable)` for joins, aliases, and aggregates.
The adapter does not contain application table names, column names, or business
rules.

```rust
use gproxy_seaorm::{D1Type, Projection};

let projection = Projection::new()
    .column("total", D1Type::I64, false)?;
let aggregate = connection.with_projection(projection);
```

Named decoding is the default, supporting entities, `FromQueryResult`, and
`try_get("", "alias")`. Decoded rows are keyed by alias and sorted. For
`into_tuple()` or `try_get_by_index()`, call `.by_index()` on the projection.
That mode preserves the SQL column order and does not support reading by the
original column names. Index-based reads in the default named mode would follow
sorted alias order instead of SQL column order.

Duplicate or unknown aliases, mismatched row widths, invalid column values, and
unexpected NULLs return errors. Queries with no rows return an empty collection.

## Portable batches

`BatchConnectionTrait` is implemented for native SeaORM connections and
`LibsqlConnection`. Ordinary operations continue to use `ConnectionTrait`.
Batch SQL can be generated with SeaORM/SeaQuery; the adapter has no business tables.

Call `batch_owned` or `atomic_batch_owned` when the statements were just built and
the caller no longer needs them. Native adapters can move their parameters into
the execution queue instead of cloning them. The borrowed forms remain available.

For one query, `query_rows(query.batch_query_for(&connection)?)` avoids an extra
native transaction and skips projection construction on native drivers.
libSQL retains the projection it needs to decode remote rows. Use `query_batch`
when multiple reads must share a snapshot.

With the native `group-commit` feature, `group::install(&connection)` puts SQLite
queries and write batches submitted through this trait in one ordered queue.
Mixed reads/writes are dispatched as one worker command, with parameters bound
independently and one result per input statement. Each individual SQL template
reuses the driver's bounded prepared-statement cache; no scripts are concatenated.
Callers encode parameters and prepare retry data before enqueueing. The drainer
moves ready commands into a pre-sized batch rather than scanning SQL or cloning
argument vectors. SQL text and parameter text/blob payloads are shared; normal
completion releases the retained payloads on the caller's task.
A lone ordinary query avoids BEGIN/COMMIT; grouped jobs share a transaction. A
failed group rolls back before each job is retried alone. Scripts and numbered
parameters keep SQLx's native per-statement binding scope, and a script still
produces one batch result (combined execution metadata or collected rows).

This uses `SqliteConnection::execute_batch`, which is not in a SQLx release yet
(proposed in transact-rs/sqlx#4511). The root Cargo patch selects SQLx 0.9.0 with
that commit from a fork; see the comment there.

| Method | Input | Result |
|---|---|---|
| `atomic_batch` | Insert/update/delete statements | An `ExecResult` per statement |
| `query_batch` | Queries, each with its own named `Projection` | A row set per query, including empty sets |
| `batch` | Ordered `Execute` and `Query` steps | Matching `BatchResult` variants in input order |

All three methods submit one transaction. Native connections use SeaORM transactions
with repeatable-read isolation (SQLite uses its native transaction snapshot);
libSQL uses one pipeline call. Native applications enable their desired SeaORM driver
features. Query steps can use SELECT or backend-supported DML RETURNING. Execute
steps return execution metadata, not returned rows. Empty batches do no I/O.

```rust
use gproxy_seaorm::{
    BatchConnectionTrait, BatchQuery, BatchResult, BatchStatement, D1Type, Projection,
};
use sea_orm::{DbBackend, Statement};

// SQLite SQL for either a native SQLite connection or libSQL.
let steps = [
    BatchStatement::Execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE counters SET value = value + ? WHERE id = ?",
        [1.into(), 7.into()],
    )),
    BatchStatement::Query(BatchQuery::new(
        Statement::from_sql_and_values(
            DbBackend::Sqlite, "SELECT value FROM counters WHERE id = ?", [7.into()],
        ),
        Projection::new().column("value", D1Type::I64, false)?,
    )),
];
let results = connection.batch(&steps).await?;
if let BatchResult::Rows(rows) = &results[1] {
    let value: i64 = rows[0].try_get("", "value")?;
}
```

Each query gets its own projection; the connection's default projection is
irrelevant to batch queries. Native drivers decode rows using their native
metadata.

Dialect and projection mode are checked across the batch before execution.
`max_bind_parameters()` returns `Some(32_766)` for libSQL and `None` when this
adapter does not specify a native driver limit. Large CRUD
operations may be built as several SQL statements inside one batch; this library
does not silently split one atomic batch into separate transactions or split SQL.

A SQL failure rolls back transactional DML in the batch. A conditional UPDATE
changing zero rows is successful SQL; business conditions/consumption receipts
must also guard dependent writes. Use transaction-compatible statements: native
DDL that implicitly commits and explicit transaction-control SQL are outside this
contract. No automatic retries or application transaction callbacks are exposed.
A timeout, cancellation, commit acknowledgement or result-decoding error after
dispatch does not prove rollback. Recover durable operations before retrying.

## libSQL / Turso

Feature `libsql` adds `LibsqlConnection`: the same `ConnectionTrait`,
`BatchConnectionTrait` and schema sync over the Hrana HTTP pipeline
(`POST {base}/v2/pipeline`) of a libSQL server or Turso database. The
connection encodes the protocol; the HTTP leg is a caller-supplied
`LibsqlTransport` (`Send + Sync`; gproxy-client implements it for its `Client`
behind its own `libsql` feature).

```rust
let db = LibsqlConnection::new(transport, "libsql://db.turso.io", Some(token))?;
let store = Store::new(db);
```

Writes are one pipeline batch: `BEGIN`, each statement conditioned on the
previous one succeeding, `COMMIT` conditioned on the last, and a `ROLLBACK`
conditioned on its failure, so the server never keeps a partial batch. Integers
travel as exact decimal text, blobs as base64, and results decode through the
connection's projection. `LibsqlConnection` does not implement
`TransactionTrait`; interactive transactions and ORM cascading writes requiring
that trait are unavailable.

SeaORM 2.0.3's `proxy` feature does not compile next to `sqlx-sqlite`, so the
libSQL connection materialises rows through SeaORM's mock driver, and schema
planning uses mock acknowledgements for its DDL recorder. Application
queries/batches on SQLx use the real driver.

## Types

- Projected connections support booleans, signed and unsigned integers, finite
  floating-point values, text, BLOBs, JSON, and nullable fields. `U64` results
  are limited to the signed 64-bit range. Dedicated date, time, decimal,
  UUID, and array SQL types are not converted automatically; applications can
  use explicit text, integer, or JSON representations.
- Ordinary integer parameters and numeric results must fit within
  `[-(2^53-1), 2^53-1]`. Parameters outside that range are rejected before writing.
  For the full `i64` range, bind **decimal text with `CAST(? AS INTEGER)`**, then
  read `CAST(column AS TEXT) AS alias` through an integer projection. This
  preserves both the SQL type and integer precision.
- BLOBs travel as base64, including empty and arbitrary byte sequences. JSON
  is explicitly serialized to and parsed from text.
- This crate does not provide native database drivers, Redis, KV, file
  storage, or background tasks.

## Portable JSON string membership

`json_array_contains_text(backend, array_expr, value_expr)` returns
`Result<Expr, DbErr>` for exact top-level string membership. Arguments remain
bound expressions; NULL/non-array documents and non-string elements do not match.
SQLite/libSQL use `json_each`, PostgreSQL uses `jsonb_array_elements`, and MySQL 8+
uses `JSON_TABLE`. Unknown backends return an error before SQL dispatch. The
open-source SeaORM 2.0.3 dependency has no MSSQL backend; SQL Server support is
provided separately by [SeaORM X](https://www.sea-ql.org/SeaORM-X/).

## Exact fixed-point values

`FixedDecimal` provides signed i64 atoms at scale 9, exact parsing, checked
arithmetic and explicit ties-to-even rounding from `rust_decimal::Decimal`.
Its JSON representation is a decimal string. Entity fields must declare
`column_type = "BigInteger", select_as = "char(32)", save_as = "decimal(20,0)"`.
SQL stores integers; text bindings and SELECT casts preserve full i64 precision
across JSON row transports. SeaORM column comparisons apply the save cast; manual SQL/col_expr
must apply it explicitly. Nullable fields use the same annotations.

This is an explicit custom representation, not automatic SQL DECIMAL support.
Its complete range is ±about 9.22 billion units. Parsing rejects excess precision;
round the complete calculated charge once with `FixedDecimal::rounded` when that
is the caller's policy. SQLite execution is verified; PostgreSQL
and MySQL cast generation is checked without live database validation.

## Portable schema synchronization

`SchemaSyncConnectionTrait` exposes `schema_registry()` and
`sync_schema(registry).await`, returning `SyncReport`. Its `EntityRegistry`
associated builder lets applications reuse one registration function for native
SeaORM `SchemaBuilder` and the libSQL `SchemaSync`:

```rust
use gproxy_seaorm::{EntityRegistry, SchemaSyncConnectionTrait};

fn entities<R: EntityRegistry>(registry: R) -> R {
    registry.register(provider::Entity).register(credential::Entity)
}
let report = connection.sync_schema(entities(connection.schema_registry())).await?;
```

Native connections implement SeaORM/sea-schema connection traits and use the
upstream `schema-sync` feature; drivers remain caller-selected. libSQL uses the
discovery/planning/atomic-batch path below, and its warnings are returned in
`report.warnings`; native SeaORM diagnostics go to logging. Sync is explicit,
requires a single schema writer, does not seed data, and does not replace data
migrations.

## Entity-first schema sync on libSQL

```rust
use gproxy_seaorm::schema::SchemaSync;

let changes = SchemaSync::new()
    .register(provider::Entity)
    .register(credential::Entity)
    .sync(&connection)
    .await?;
```

SeaORM generates entity DDL and dependency order. The adapter discovers existing
structures through `sqlite_schema`, `pragma_table_xinfo`, and index queries,
then executes the generated SQL in one batch. SeaORM 2.0.3's native
`SchemaBuilder::sync` discovery path depends on SQLx/rusqlite, which a remote
connection does not have.

Synchronization follows SeaORM's SQLite rules: add missing tables and columns,
handle `renamed_from`, create missing indexes, and remove unique indexes no
longer declared by the entities. Existing column type differences are reported
in `changes.warnings`; their types are not altered. Changes to existing foreign
keys, column removal, and other structural changes use migrations. Unsupported
SQLite ALTER/DROP operations return database errors, such as attempts to drop an
automatic index backing an inline UNIQUE constraint.

Use `.plan(&connection).await?` to inspect the SQL before calling
`plan.apply(&connection).await?`.

## Official SeaORM migrations

Define application migrations with the official `sea-orm-migration`
`MigrationTrait` and `MigratorTrait`. The dependency is re-exported as
`gproxy_seaorm::sea_orm_migration`.

```rust
connection.run_migrations::<Migrator>(None).await?;
let report = connection.migration_report::<Migrator>().await?;
```

`run_migrations` hands a native connection to the official runner, retaining
`seaql_migrations`, custom migration table names, version order and step
limits. A libSQL connection refuses it: migrate that database through a native
connection. `migration_report` only reads the ledger and works on every
connection.

The official `SchemaManager` inspection methods depend on native driver
features. Migrations that must also run without them can use the corresponding
extension methods while continuing to use the official SchemaManager for DDL:

```rust
use gproxy_seaorm::D1SchemaManagerExt;
use gproxy_seaorm::sea_orm_migration::sea_query::{ColumnDef, Table};

if !manager.d1_has_column("providers", "description").await? {
    manager.alter_table(
        Table::alter()
            .table("providers")
            .add_column(ColumnDef::new("description").text().null())
            .to_owned(),
    ).await?;
}
```

## Validation

```bash
cargo test -p gproxy-seaorm --all-features
cargo clippy -p gproxy-seaorm --all-features --all-targets -- -D warnings
```

Tests cover entity metadata, conversion errors, BLOBs, integer boundaries,
result order, schema sync, the libSQL pipeline encoding, and real SQLite batch
CRUD, RETURNING, mixed reads/writes, rollback and conditional-write semantics.
