# gproxy-seaorm-d1

English | [简体中文](README.zh-CN.md)

A standalone SeaORM 2 adapter for Cloudflare D1. Application entities stay in
your application. This crate provides the Workers WASM binding, parameter and
result conversion, ORM queries, D1 batches, entity-first schema synchronization,
and integration with the official `sea-orm-migration` crate. It does not depend
on gproxy core.

## Entity queries

`D1Connection` is available on `wasm32` and implements SeaORM's `ConnectionTrait`.
Pass the actual `env.DB` binding as a `JsValue`, then select an entity or explicit
result projection for the operation:

```rust
use gproxy_seaorm_d1::D1Connection;
use sea_orm::EntityTrait;

let connection = D1Connection::from_binding(binding)?;
let providers = connection.for_entity::<provider::Entity>()?;
let rows = provider::Entity::find().all(&providers).await?;
```

`for_entity` derives types and nullability from `Column::def()`. Each connection
view owns its projection; tables do not share a mutable column-name registry.
When overriding entity SQL column types or using custom Rust value types, check
that the column metadata matches the Rust decoding type. Use an explicit
projection when they differ.

## Custom queries and result order

Use `Projection::column(alias, type, nullable)` for joins, aliases, and aggregates.
The adapter does not contain application table names, column names, or business
rules.

```rust
use gproxy_seaorm_d1::{D1Type, Projection};

let projection = Projection::new()
    .column("total", D1Type::I64, false)?;
let aggregate = connection.with_projection(projection);
```

Named decoding is the default, supporting entities, `FromQueryResult`, and
`try_get("", "alias")`. SeaORM's `ProxyRow` sorts its keys internally. For
`into_tuple()` or `try_get_by_index()`, call `.by_index()` on the projection.
That mode preserves the SQL column order returned by D1 and does not support
reading by the original column names. Index-based reads in the default named
mode would follow sorted alias order instead of SQL column order.

The connection uses D1's `raw({columnNames: true})` to obtain column names and
ordered values. Duplicate or unknown aliases, mismatched row widths, invalid
column values, and unexpected NULLs return errors. Queries with no rows return
an empty collection.

## Atomic writes

```rust
let results = connection.atomic_batch(&statements).await?;
let changed = results[0].rows_affected();
```

Statements must use the SQLite dialect. Parameters are converted before the
batch is sent. `atomic_batch` returns execution metadata for each statement;
it does not decode SELECT or RETURNING rows. Use a connection with a projection
for queries and ORM RETURNING operations. Affected-row counts use D1's
`meta.changes`, rather than `rows_written`, which includes index writes.

The application connection **does not implement `TransactionTrait` or expose an
underlying `DatabaseConnection`**. Interactive `begin()/rollback()` calls and ORM
cascading writes requiring that trait are unavailable. D1 rejects unsupported
transaction-control SQL.

A D1 batch rolls back when a SQL statement fails. A conditional UPDATE affecting
zero rows is still successful SQL, so business preconditions must also govern
dependent writes. A timeout, cancellation, or result-decoding failure after
dispatch does not prove that writes were undone. Recover using a durable
operation ID instead of automatically repeating non-idempotent writes.

## Types and runtime

- Supports booleans, signed and unsigned integers, finite floating-point values,
  text, BLOBs, JSON, and nullable fields. D1 stores signed 64-bit integers, so
  `U64` results are also limited to that range. Dedicated date, time, decimal,
  UUID, and array SQL types are not converted automatically; applications can
  use explicit text, integer, or JSON representations.
- Ordinary integer parameters and numeric results must fit within
  `[-(2^53-1), 2^53-1]`. Parameters outside that range are rejected before writing.
  For the full `i64` range, bind **decimal text with `CAST(? AS INTEGER)`**, then
  read `CAST(column AS TEXT) AS alias` through an integer projection. This
  preserves both the SQL type and integer precision.
- BLOB parameters use ArrayBuffer, including empty and arbitrary byte sequences.
  JSON is explicitly serialized to and parsed from text.
- `SendWrapper` keeps JS handles and futures on their originating thread. The
  crate does not add its own `unsafe impl Send/Sync` implementations.
- `Projection` can be constructed and tested on native targets. The actual D1
  connection is available only on Workers WASM. This crate does not provide
  native database drivers, Redis, KV, file storage, or background tasks.

## Entity-first schema sync

```rust
let changes = connection.schema_sync()
    .register(provider::Entity)
    .register(credential::Entity)
    .sync(&connection)
    .await?;
```

SeaORM generates entity DDL and dependency order. The adapter discovers existing
structures through D1-supported `sqlite_schema`, `pragma_table_xinfo`, and index
queries, then executes the generated SQL in a D1 batch. SeaORM 2.0.3's native
`SchemaBuilder::sync` discovery path depends on SQLx/rusqlite; this adapter
provides the D1 entry point without linking a native SQLite driver into WASM.

Synchronization follows SeaORM's SQLite rules: add missing tables and columns,
handle `renamed_from`, create missing indexes, and remove unique indexes no
longer declared by the entities. Existing column type differences are reported
in `changes.warnings`; their types are not altered. Changes to existing foreign
keys, column removal, and other structural changes use migrations. Unsupported
SQLite ALTER/DROP operations return database errors, such as attempts to drop an
automatic index backing an inline UNIQUE constraint.

Use `.plan(&connection).await?` to inspect the SQL before calling
`plan.apply(&connection).await?`. To inspect table, column, and index metadata,
call `connection.inspect_schema(&["providers"]).await?`.

## Official SeaORM migrations

Define application migrations with the official `sea-orm-migration`
`MigrationTrait` and `MigratorTrait`. This adapter uses its migration format and
history table. The dependency is also re-exported as
`gproxy_seaorm_d1::sea_orm_migration`.

```rust
connection.migrate_up::<Migrator>(None).await?;
let status = connection.migration_status::<Migrator>().await?;
connection.migrate_down::<Migrator>(Some(1)).await?;
```

These methods call the official migrator, retaining `seaql_migrations`, custom
migration table names, version order, step limits, skipping applied versions,
and down operations. Migration timestamps use a WASM-compatible clock.

The official `SchemaManager` inspection methods depend on native driver
features. D1 migrations can use the corresponding extension methods while
continuing to use the official SchemaManager for DDL:

```rust
use gproxy_seaorm_d1::D1SchemaManagerExt;
use gproxy_seaorm_d1::sea_orm_migration::sea_query::{ColumnDef, Table};

if !manager.d1_has_column("providers", "description").await? {
    manager.alter_table(
        Table::alter()
            .table("providers")
            .add_column(ColumnDef::new("description").text().null())
            .to_owned(),
    ).await?;
}
```

D1 follows SeaORM's default SQLite behavior: statements commit individually.
Failed migrations are not recorded as applied, but earlier successful DDL/DML
may already have committed. Inspect and repair the migration before retrying.
Requests for interactive transactions fail instead of treating SeaORM's empty
Proxy transaction hooks as successful transactions. The application connection
still does not implement `TransactionTrait`.

## Validation

```bash
cargo test -p gproxy-seaorm-d1
cargo clippy -p gproxy-seaorm-d1 --all-targets -- -D warnings
cargo clippy -p gproxy-seaorm-d1 --target wasm32-unknown-unknown -- -D warnings
```

Native tests cover entity metadata, conversion errors, BLOBs, integer boundaries,
result order, logical affected-row counts, and schema sync rules. See [VALIDATION.md](VALIDATION.md)
for live validation notes in Chinese. Temporary deployment scripts and test
Workers are not included in the library.
