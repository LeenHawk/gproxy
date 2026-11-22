# gproxy-seaorm

[English](README.md) | 简体中文

SeaORM 2 批量操作与 libSQL／Turso 的独立适配 crate。业务 entity 留在调用方；本库负责投影连接的
参数和结果映射、跨后端原子 batch、entity-first sync，以及官方 `sea-orm-migration` 的接入，
不依赖 gproxy core。

## 普通 entity 查询

启用 `libsql` feature 后，`LibsqlConnection` 实现 SeaORM `ConnectionTrait`；远端返回的行
没有类型，需要为本次操作选择 entity 或显式结果投影：

```rust
use gproxy_seaorm::LibsqlConnection;
use sea_orm::EntityTrait;

let connection = LibsqlConnection::new(transport, "libsql://db.turso.io", Some(token))?;
let providers = connection.for_entity::<provider::Entity>()?;
let rows = provider::Entity::find().all(&providers).await?;
```

`for_entity` 从 `Column::def()` 提取类型和 nullability。连接视图独立持有投影，多个表
不共享可变的列名注册表。显式覆盖 entity SQL 列类型、使用自定义 Rust 值类型时，应确认
列元数据与 Rust 解码类型一致；不一致则使用显式投影。

## 自动生成实体与关联投影

导入 `SelectProjection` 后，可从普通 SeaORM select 直接生成结果投影。支持单表、
两表可选／必选／分组关联，以及三至六表 select；一对多、多对多继续使用 SeaORM 的
关系定义和结果分组，不另建查询框架。

```rust
use gproxy_seaorm::{BatchConnectionTrait, SelectProjection};
use sea_orm::{DbBackend, EntityTrait};

let query = provider::Entity::find().find_with_related(credential::Entity);

// 单条查询：仍返回 SeaORM 的 (provider, Vec<credential>)。
let view = connection.for_select(&query)?;
let models = query.clone().all(&view).await?;

// 通用批量查询：每个查询返回对应的平面行集合。
let prepared = query.batch_query(DbBackend::Sqlite)?;
let result_sets = connection.query_batch(&[prepared]).await?;
```

辅助接口自动处理 `A_`、`B_`、`C_` 等别名；LEFT JOIN 的可选关联实体全部列（包括主键）
允许 NULL，必选 INNER JOIN 保留 entity 声明的可空性。支持改名后的 SQL 列和标准
ActiveEnum 查询。过滤、排序、LIMIT/OFFSET、选择普通列的子集无需手写映射；未选中的
不支持类型不会妨碍已选列的投影生成。

WHERE 条件不受选择列投影限制：AND／OR、IN、LIKE、比较、NULL 判断、关联列筛选及
EXISTS 子查询均由 SeaORM 原样生成参数化 SQL。需要显式投影的聚合／自定义表达式指
SELECT 的返回列，不是 WHERE 中的条件。

先构造最终选择列，再生成连接视图／批量请求。三／四表分组包装类型未暴露 SeaORM
QueryTrait，应在转成分组包装前从普通 select 生成投影。分页器的 count 查询是单独的
聚合查询，需要单独提供显式投影。

聚合表达式、自定义列别名／表别名（包括引入别名的 linked／自关联查询）、自定义 select
类型转换（entity 未声明的）、重复结果别名和不支持的已选列类型，会在自动生成阶段、执行 SQL 前报错。
这些场景使用 `BatchQuery::new(statement, projection)` 或 `with_projection`。自动生成
不解析任意 SQL，也不根据输出别名猜聚合类型。entity 声明的 `select_as` 可按完整表达式
精确匹配，沿用该列元数据。

手工关联完整实体列集合时，可用 `Projection::for_entity_prefixed::<E>(prefix, nullable)`
和 `.merge(...)` 组合；nullable 表示整个关联侧可为空。解码检查仍保留，缺失的
投影不会被静默猜测。

## 自定义查询与结果顺序

JOIN、别名、聚合通过 `Projection::column(alias, type, nullable)` 指定结果类型。
数据库中的业务表名、字段名和业务规则不进入适配器。

```rust
use gproxy_seaorm::{D1Type, Projection};

let projection = Projection::new()
    .column("total", D1Type::I64, false)?;
let aggregate = connection.with_projection(projection);
```

默认使用列名解码，适用于 entity、`FromQueryResult` 和 `try_get("", "alias")`。
解码后的行按别名排序；使用 `into_tuple()` 或 `try_get_by_index()` 时，必须
在投影上调用 `.by_index()`。该模式按 SQL 列顺序建立索引，不能再按原列名读。
不要在默认命名模式下使用下标读取，否则 SeaORM 会按别名排序后的顺序读取。

重复/未知别名、行宽不匹配、不符合列类型的值和非空列中的 NULL 都会报错。查询空结果返回空集合。

## 统一批量接口

`BatchConnectionTrait` 同时实现于原生 SeaORM 连接和 `LibsqlConnection`。
普通操作继续使用 `ConnectionTrait`；批量 SQL 由 SeaORM／SeaQuery
构造，本库不包含业务表名和业务事务规则。

刚构造好且不再需要保留的语句，可以交给 `batch_owned` 或 `atomic_batch_owned`。
原生适配器能够把参数直接移入执行队列，避免再次克隆；原来的借用接口仍然可用。

单次查询使用 `query_rows(query.batch_query_for(&connection)?)`，原生驱动不会额外开启
事务，也不会构造它用不到的投影。libSQL 仍保留远端解码所需的类型信息。
多次读取需要共享快照时，继续使用 `query_batch`。

原生 `group-commit` feature 下，`group::install(&connection)` 将本接口发起的 SQLite 查询和写入批次
放入同一个有序队列。异构读写以有界、保留参数绑定的命令执行，并保留逐语句结果。
只有相同 SQL 结构进入长期语句缓存，动态混合脚本不缓存。单独的普通查询不额外执行
BEGIN/COMMIT，合组任务共享事务；合组失败先回滚，再逐任务执行，隔离各自的失败。

| 方法 | 输入 | 返回 |
|---|---|---|
| `atomic_batch` | 批量增删改 SQL | 按输入顺序返回每条语句的 ExecResult |
| `query_batch` | 每条带独立命名 Projection 的查询 | 每个查询一个结果集，空结果保留为空集合 |
| `batch` | 有序 Execute／Query 步骤 | 与输入步骤对应的 BatchResult |

三种方法都在一个事务中执行。原生连接使用 SeaORM 事务与 repeatable-read 隔离
（SQLite 使用原生事务快照），libSQL 使用一次 pipeline 调用。原生应用自行开启所需 SeaORM
驱动 feature。Query 步骤支持 SELECT 和后端支持的 DML RETURNING；Execute 返回执行
元数据、不返回行。空批次不执行 I/O。

```rust
use gproxy_seaorm::{
    BatchConnectionTrait, BatchQuery, BatchResult, BatchStatement, D1Type, Projection,
};
use sea_orm::{DbBackend, Statement};

// SQLite SQL，可用于原生 SQLite 或 libSQL。
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

每条查询使用自己的 Projection，不依赖连接默认投影；原生驱动根据自己的结果元数据解码。

整批执行前检查 SQL 方言和投影模式。`max_bind_parameters()`
在 libSQL 返回 `Some(32_766)`，原生驱动上本库未指定限制时返回 None。大量 CRUD 可生成多条 SQL
放进同一个 batch；库不会自动拆 SQL，也不会把一次原子操作悄悄拆成多个事务。

SQL 失败会回滚同批的事务性 DML。条件更新影响零行仍是成功，后续写入必须受业务条件／
消费凭据约束。原生数据库会隐式提交的 DDL、显式事务控制 SQL 不在该原子保证内。
不提供自动重试或交互式业务闭包。派发后的超时、取消、提交确认或结果解码错误不证明
写入回滚，重试前应查询持久操作结果。

## libSQL／Turso

`libsql` feature 提供 `LibsqlConnection`：通过 libSQL 服务器或 Turso 的 Hrana HTTP pipeline
（`POST {base}/v2/pipeline`）实现同样的 `ConnectionTrait`、`BatchConnectionTrait` 和 schema
同步。连接负责协议编码，HTTP 由调用方提供的 `LibsqlTransport`（`Send + Sync`）发送；
gproxy-client 在自己的 `libsql` feature 下为 `Client` 实现了它。

写入是一次 pipeline batch：`BEGIN`，每条语句以前一条成功为条件，`COMMIT` 以最后一条成功
为条件，失败时执行 `ROLLBACK`，服务器不会留下半个批次。整数以精确十进制文本传输，BLOB 用
base64。`LibsqlConnection` 不实现 `TransactionTrait`；交互式事务以及依赖该 trait 的 ORM
级联操作不可用。

SeaORM 2.0.3 的 `proxy` feature 无法与 `sqlx-sqlite` 一起编译，因此 libSQL 连接通过 SeaORM
mock 驱动构造结果行，schema 规划用 mock 回执记录生成的 DDL；SQLx 上的业务查询和批量操作
仍走实际数据库驱动。

## 类型边界

- 投影连接支持 bool、有符号/无符号整数、有限浮点数、文本、BLOB、JSON、可空字段。
  `U64` 结果受有符号 64 位范围限制。日期、时间、decimal、UUID、数组等
  专用 SQL 类型不自动转换；可在业务模型中使用明确的文本/整数/JSON 表示。
- 普通整数参数和数值结果必须位于 `[-(2^53-1), 2^53-1]`。超界参数在写入前拒绝。
  完整 i64 使用十进制**字符串参数 + `CAST(? AS INTEGER)`**，读取用
  `CAST(column AS TEXT) AS alias` 和整数类型投影，从而保持 SQL 类型及精度。
- BLOB 以 base64 传输，支持空值和任意 byte；JSON 明确序列化/解析文本。
- 本库不提供原生数据库驱动、Redis、KV、文件存储或后台任务。

## 跨后端 JSON 字符串匹配

`json_array_contains_text(backend, array_expr, value_expr)` 返回 `Result<Expr, DbErr>`，
在 JSON 数组顶层进行精确字符串匹配；参数保持绑定表达式，NULL／非数组值和非字符串
元素不匹配。SQLite／libSQL 使用 `json_each`，PostgreSQL 使用 `jsonb_array_elements`，
MySQL 8+ 使用 `JSON_TABLE`；未知后端在派发 SQL 前明确报错。当前开源 SeaORM 2.0.3
依赖没有 MSSQL 后端，SQL Server 支持由单独的 [SeaORM X](https://www.sea-ql.org/SeaORM-X/)
提供，未集成到本项目。

## 精确定点数

`FixedDecimal` 提供固定 9 位小数的 i64 原子值、精确解析、带溢出检查的加减，以及从
`rust_decimal::Decimal` 显式中点取偶舍入。JSON 表示为十进制字符串。实体字段必须声明
`column_type = "BigInteger", select_as = "char(32)", save_as = "decimal(20,0)"`。
SQL 存整数，文本参数和查询转换在 JSON 行传输中保持完整 i64 精度。SeaORM 列比较会应用保存转换；
手写 SQL／col_expr 必须显式应用。可空字段使用相同标注。

这是明确的自定义表示，不是 SQL DECIMAL 自动支持。可表示约正负 92.2 亿单位。
解析拒绝超出精度；需要舍入时对计算完成的整笔费用调用 `FixedDecimal::rounded` 一次。
已验证 SQLite 执行；PostgreSQL／MySQL 仅检查转换 SQL 生成，未运行真实数据库。

## 统一 schema 同步接口

`SchemaSyncConnectionTrait` 提供 `schema_registry()` 与 `sync_schema(registry).await`，
返回 `SyncReport`。关联 builder 实现 `EntityRegistry`，同一个注册函数可用于原生
SeaORM `SchemaBuilder` 与 libSQL 的 `SchemaSync`：

```rust
use gproxy_seaorm::{EntityRegistry, SchemaSyncConnectionTrait};

fn entities<R: EntityRegistry>(registry: R) -> R {
    registry.register(provider::Entity).register(credential::Entity)
}
let report = connection.sync_schema(entities(connection.schema_registry())).await?;
```

原生连接实现 SeaORM／sea-schema 连接 trait，使用上游 `schema-sync`；数据库驱动仍由
调用方选择。libSQL 复用下述结构发现／规划／原子 batch 路径，警告返回在 `report.warnings`，
原生 SeaORM 诊断通过日志输出。同步须显式执行、由单个 schema 写入者协调，不初始化业务
数据，也不替代数据迁移。

## libSQL 上的 entity-first sync

```rust
use gproxy_seaorm::schema::SchemaSync;

let changes = SchemaSync::new()
    .register(provider::Entity)
    .register(credential::Entity)
    .sync(&connection)
    .await?;
```

复用 SeaORM 的 entity DDL 生成和依赖排序，使用 `sqlite_schema`、
`pragma_table_xinfo` 和索引查询读取现有结构，再用一次 batch 执行生成的 SQL。
SeaORM 2.0.3 原生 `SchemaBuilder::sync` 的发现路径依赖 SQLx/rusqlite，远端连接没有这些驱动。

同步规则沿用 SeaORM 的 SQLite 行为：补表和列，处理 `renamed_from`，创建缺失索引，
删除 entity 中已移除的唯一索引。已有列的类型差异返回 `changes.warnings`，不修改其类型；
已有表的外键、列删除及其他结构变化通过迁移处理。SQLite 不允许的 ALTER/DROP 操作直接
返回数据库错误，例如移除内联 UNIQUE 的自动索引。

可用 `.plan(&connection).await?` 查看 SQL，再调用 `plan.apply(&connection).await?`。

## 官方 SeaORM migration

业务迁移按 `sea-orm-migration` 的 `MigrationTrait`、`MigratorTrait` 定义，本库不另建迁移
格式或历史表。该依赖也通过 `gproxy_seaorm::sea_orm_migration` 重新导出。

```rust
connection.run_migrations::<Migrator>(None).await?;
let report = connection.migration_report::<Migrator>().await?;
```

`run_migrations` 把原生连接交给官方 migrator，保留 `seaql_migrations`、自定义迁移表名、
版本顺序和 steps。libSQL 连接会拒绝该调用：请通过原生连接迁移。`migration_report`
只读取历史表，所有连接都可用。

官方 `SchemaManager` 的 `has_table/has_column/has_index` 依赖原生驱动 feature；需要在没有
这些 feature 时运行的迁移可以使用对应扩展，其他 DDL 仍使用官方 SchemaManager：

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

## 验证

```bash
cargo test -p gproxy-seaorm --all-features
cargo clippy -p gproxy-seaorm --all-features --all-targets -- -D warnings
```

测试覆盖类型元数据、错误拒绝、BLOB、整数边界、结果顺序、schema 同步规则、libSQL pipeline
编码，以及真实 SQLite 批量操作。
