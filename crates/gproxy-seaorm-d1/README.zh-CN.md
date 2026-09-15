# gproxy-seaorm-d1

[English](README.md) | 简体中文

SeaORM 2 到 Cloudflare D1 的独立适配 crate。业务 entity 留在调用方；本库负责 Workers
WASM binding、参数和结果映射、普通 ORM 操作、D1 batch、entity-first sync，以及官方
`sea-orm-migration` 的接入，不依赖 gproxy core。

## 普通 entity 查询

`D1Connection` 在 `wasm32` 上提供，实现 SeaORM `ConnectionTrait`。传入实际 `env.DB`
binding 的 `JsValue`，再为本次操作选择 entity 或显式结果投影：

```rust
use gproxy_seaorm_d1::D1Connection;
use sea_orm::EntityTrait;

let connection = D1Connection::from_binding(binding)?;
let providers = connection.for_entity::<provider::Entity>()?;
let rows = provider::Entity::find().all(&providers).await?;
```

`for_entity` 从 `Column::def()` 提取类型和 nullability。连接视图独立持有投影，多个表
不共享可变的列名注册表。显式覆盖 entity SQL 列类型、使用自定义 Rust 值类型时，应确认
列元数据与 Rust 解码类型一致；不一致则使用显式投影。

## 自定义查询与结果顺序

JOIN、别名、聚合通过 `Projection::column(alias, type, nullable)` 指定结果类型。
数据库中的业务表名、字段名和业务规则不进入适配器。

```rust
use gproxy_seaorm_d1::{D1Type, Projection};

let projection = Projection::new()
    .column("total", D1Type::I64, false)?;
let aggregate = connection.with_projection(projection);
```

默认使用列名解码，适用于 entity、`FromQueryResult` 和 `try_get("", "alias")`。
SeaORM 的 ProxyRow 内部按键排序；使用 `into_tuple()` 或 `try_get_by_index()` 时，必须
在投影上调用 `.by_index()`。该模式按 D1 返回的 SQL 列顺序建立索引，不能再按原列名读。
不要在默认命名模式下使用下标读取，否则 SeaORM 会按别名排序后的顺序读取。

连接通过 D1 `raw({columnNames: true})` 获取列名及有序值，拒绝重复/未知别名、行宽不匹配、
不符合列类型的值和非空列中的 NULL。查询空结果返回空集合。

## 原子写入

```rust
let results = connection.atomic_batch(&statements).await?;
let changed = results[0].rows_affected();
```

所有语句必须是 SQLite 方言，全部参数通过验证后才发送。`atomic_batch` 返回每条语句的
执行元数据，不解码 SELECT/RETURNING 行；普通查询与 ORM RETURNING 使用带投影的连接。
影响行数取 D1 的 `meta.changes`，不使用包含索引写入的 `rows_written`。

本类型**不实现 `TransactionTrait`，不返回底层 `DatabaseConnection`**。不能调用
`begin()/rollback()` 或依赖交互式事务的 ORM 级联写入；原始 SQL 的事务指令由 D1 拒绝。

D1 batch 只有 SQL 失败才整批回滚。条件 UPDATE 影响 0 行仍是成功，业务前置条件必须
控制所有后续写入。派发后的超时、取消或返回值解码失败不证明写入未发生；调用方应通过
持久化操作 ID 查询结果，不自动重发非幂等写入。

## 类型与运行边界

- 支持 bool、有符号/无符号整数、有限浮点数、文本、BLOB、JSON、可空字段。D1 的整数
  存储是有符号 64 位，`U64` 结果也受该范围限制。日期、时间、decimal、UUID、数组等
  专用 SQL 类型不自动转换；可在业务模型中使用明确的文本/整数/JSON 表示。
- 普通整数参数和数值结果必须位于 `[-(2^53-1), 2^53-1]`。超界参数在写入前拒绝。
  完整 i64 使用十进制**字符串参数 + `CAST(? AS INTEGER)`**，读取用
  `CAST(column AS TEXT) AS alias` 和整数类型投影，从而保持 SQL 类型及精度。
- BLOB 参数传 ArrayBuffer，支持空值和任意 byte；JSON 明确序列化/解析文本。
- `SendWrapper` 保证 JS handle 和 future 只在创建它的线程访问，没有自行编写
  `unsafe impl Send/Sync`。
- `Projection` 在原生平台也可构造和测试；真正的 D1 连接只在 Workers WASM 上提供。
  本库不提供原生数据库驱动、Redis、KV、文件存储或后台任务。

## Entity-first sync

```rust
let changes = connection.schema_sync()
    .register(provider::Entity)
    .register(credential::Entity)
    .sync(&connection)
    .await?;
```

复用 SeaORM 的 entity DDL 生成和依赖排序，使用 D1 支持的 `sqlite_schema`、
`pragma_table_xinfo` 和索引查询读取现有结构，再用 D1 batch 执行生成的 SQL。
SeaORM 2.0.3 原生 `SchemaBuilder::sync` 的发现路径依赖 SQLx/rusqlite，因此 D1 使用上述
适配入口，而不是给 WASM 链接原生 SQLite 驱动。

同步规则沿用 SeaORM 的 SQLite 行为：补表和列，处理 `renamed_from`，创建缺失索引，
删除 entity 中已移除的唯一索引。已有列的类型差异返回 `changes.warnings`，不修改其类型；
已有表的外键、列删除及其他结构变化通过迁移处理。SQLite 不允许的 ALTER/DROP 操作直接
返回数据库错误，例如移除内联 UNIQUE 的自动索引。

可用 `.plan(&connection).await?` 查看 SQL，再调用 `plan.apply(&connection).await?`；
也可用 `connection.inspect_schema(&["providers"]).await?` 获取表、列和索引信息。

## 官方 SeaORM migration

业务迁移按 `sea-orm-migration` 的 `MigrationTrait`、`MigratorTrait` 定义，本库不另建迁移
格式或历史表。该依赖也通过 `gproxy_seaorm_d1::sea_orm_migration` 重新导出。

```rust
connection.migrate_up::<Migrator>(None).await?;
let status = connection.migration_status::<Migrator>().await?;
connection.migrate_down::<Migrator>(Some(1)).await?;
```

这些方法调用官方 migrator，保留 `seaql_migrations`、自定义迁移表名、版本顺序、steps、
已应用版本跳过和 down 行为。迁移时间使用 WASM 可用的时钟。

官方 `SchemaManager` 的 `has_table/has_column/has_index` 依赖原生驱动 feature；D1 迁移中
可以使用对应扩展，其他 DDL 仍使用官方 SchemaManager：

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

D1 沿用官方 SQLite 默认的逐语句提交行为。迁移失败不会写入该版本记录，但此前成功的
DDL/DML 可能已经提交；应按实际结果修复迁移后再执行。迁移中请求交互式事务会失败，
不会把 SeaORM Proxy 的空事务钩子当作成功事务。普通应用连接仍不提供 `TransactionTrait`。

## 验证

```bash
cargo test -p gproxy-seaorm-d1
cargo clippy -p gproxy-seaorm-d1 --all-targets -- -D warnings
cargo clippy -p gproxy-seaorm-d1 --target wasm32-unknown-unknown -- -D warnings
```

原生测试覆盖类型元数据、错误拒绝、BLOB、整数边界、结果顺序、逻辑影响行数和 schema 同步规则。
云端验证记录见 [VALIDATION.md](VALIDATION.md)。一次性部署脚本与测试 Worker 不随库保留。
