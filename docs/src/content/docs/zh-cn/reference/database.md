---
title: "存储与缓存后端"
description: "四种 SQL 后端及其选择方式、schema 迁移与表分组、缓存后端与多实例要求、备份、保留策略，以及 Edge 的限制"
---

`gproxy-store` 为所有后端维护一份 schema 目录和一层查询构造（SeaQuery）。
方言差异——整数宽度、MySQL 上带索引列的 `VARCHAR(255)`、SQLite 上的
`PRAGMA foreign_keys`——在渲染语句时应用，后端一致性由 store 的测试场景覆
盖。缓存是独立于持久化选择的另一项服务。

## 选择后端

| `GPROXY_PERSISTENCE` | 连接 | 说明 |
| --- | --- | --- |
| `sqlite`（默认） | `<data-dir>/gproxy.db` | 内置 SQLite，单文件。启用外键。 |
| `libsql` | `GPROXY_LIBSQL_URL` + `GPROXY_LIBSQL_AUTH_TOKEN` | 通过 HTTP 的 Hrana 协议，端点为 `<url>/v2/pipeline`；适用于 Turso 和任何 libSQL 服务器。Edge 上唯一的后端。 |
| `postgres` | `GPROXY_DSN=postgres://user:<password>@host:5432/gproxy` | `tokio-postgres`，单个连接加锁串行；每个迁移批次在一个事务中执行。连接不启用 TLS；请把数据库放在私有网络或本地 socket 上。 |
| `mysql` | `GPROXY_DSN=mysql://user:<password>@host:3306/gproxy` | `mysql_async` 连接池，支持 rustls TLS；迁移批次在事务中执行。 |

```bash
GPROXY_PERSISTENCE=postgres \
GPROXY_DSN='postgres://gproxy:<password>@db.internal:5432/gproxy' \
gproxy
```

列类型：`Integer` 在 SQLite 上是 `INTEGER`，其他后端是 `BIGINT`；`Text` 是
`TEXT`，MySQL 上带索引的列是 `VARCHAR(255)`；`Blob` 是二进制，MySQL 上带索
引的列是 `VARBINARY(255)`。时间戳是整数列中的 Unix 秒，金额是小数文本，
JSON 是文本。

## 迁移

启动时先打开后端并完成迁移，再做其他任何事；没有单独的迁移命令。
`schema_migrations(version, applied_at)` 记录每个已应用的版本。历史必须连
续；数据库比二进制更新时以 `database schema is newer than this binary` 拒绝。

| 版本 | 名称 | 新增 |
| --- | --- | --- |
| 1 | `Initial` | 下文列出的完整当前 v3 schema。首个 v3 发布之前的开发期版本已被压平；发布后新增的迁移从版本 2 开始。 |

版本 1 是当前 schema。自更新 manifest 携带最低数据版本，应用更新前会与此
数字比较。

这条阶梯只升级 v3 存储。`gproxy migrate --from-v2` 是独立的数据导入器：它
只读取 v2 SQLite 源而不修改它，打开一个当前的 v3 目标（从而创建 Initial 版
本 1），再映射并写入 v2 实体。它不会重放已被取代的 v3 开发期迁移。用已移除
的 15 版本阶梯创建的 v3 预发布存储必须重建；它们从来不是受支持的迁移来源。

## 表分组

| 分组 | 表 | 说明 |
| --- | --- | --- |
| Provider 与路由 | `providers`、`credentials`、`provider_models`、`routes`、`route_members`、`exposed_models`、`aliases` | Provider、路由和公开模型名唯一。`credentials` 存放密封信封（`ciphertext`、`wrapped_key`、`payload_nonce`、`key_nonce`）和用于轮换时 compare-and-swap 的 `version`。 |
| 规则 | `routing_rules`、`rule_sets`、`rules`、`provider_rule_sets` | `routing_rules` 对 `(provider_id, operation, kind)` 唯一；`origin` 区分通道播种行与操作员行。 |
| 定价 | `price_rules`、`price_rates` | 见[价格与分层](/zh-cn/reference/pricing/)。 |
| 身份 | `organizations`、`teams`、`users`、`user_keys`、`user_sessions`、`permissions`、`rate_limits`、`quotas` | 团队名在组织内唯一。`user_keys` 保存唯一摘要、用于显示的 `prefix` 和密封的密钥。每个主体一条配额行。 |
| 配额运行时 | `quota_windows`、`quota_settlements`、`credential_quota_cycles`、`credential_quota_cycle_models` | 窗口对 `(配额, 类型, 起点)` 唯一；结算对 `(请求, 窗口)` 唯一。周期记录每个凭证的上游配额读数。 |
| 用量 | `usage_rows`、`usage_rollups` | 每个请求 ID 一行，含 token 列、`metrics_json`、`dimensions_json`、小数 `cost`、`usage_source`、`ended`、`latency_ms`。汇总对 `(granularity, bucket_start, dimension_key)` 唯一。 |
| 日志 | `request_logs`、`wire_logs` | 每个请求 ID 一条下游交换；每次上游尝试一条线路日志。正文是 blob，只在开启正文捕获时存在。 |
| 管理 | `admin_audit_events`、`credential_health`、`surface_bindings`、`settings` | 健康状态按 `(凭证, 模型)` 记录。绑定把服务面资源固定到创建它的凭证。`settings` 是键到 JSON 的映射。 |
| 分词器 | `tokenizer_vocabs`、`tokenizer_auth` | 缓存的词表和密封的 Hugging Face Token。 |
| OAuth | `oauth_grants`、`oauth_codes`、`oauth_tokens`、`oauth_devices` | 模拟厂商认证面的签发方状态。 |

## 归属关系

schema 不声明数据库外键，因为四种后端对外键的支持并不一致。取而代之的是每张表
声明自己拥有哪些行，所有删除都从这份声明生成，在一个事务里完成：删除 Provider
会带走它的凭证、路由成员、别名、模型目录、价格规则、路由规则和规则集挂载；删除
路由带走成员和公开模型；删除组织、团队、用户或密钥带走作用于它们的权限、限流和
配额；删除团队只把用户的团队字段置空。历史数据从不跟随删除：用量行、汇总、配额
周期、日志和审计事件保留已不存在主体的 ID。一个 schema 步骤会清理归属方已经
消失的行，并有测试拒绝任何既未声明归属也未列为历史的引用列。

## 缓存后端

| 后端 | 选择方式 | 范围 |
| --- | --- | --- |
| 进程内 | 原生默认 | 单进程。 |
| Redis | `GPROXY_REDIS_URL` | 共享。`redis` crate 连接管理器；rustls TLS。 |
| Upstash REST | `UPSTASH_URL` + `UPSTASH_TOKEN` | 共享。每条命令一次 HTTPS 请求；原生和 Edge 均可用。 |
| libSQL 表 | 持久化为 `libsql` 且未设置以上两者时自动启用 | 通过数据库共享：`gproxy_kv(k, v, expires_ms)`。 |

缓存契约是 `get`、`set`、`delete`、`incr`、`compare_incr_and_set` 和
`compare_and_swap`，都带可选 TTL。它承载准入状态
（`gproxy:admission:{request_id}`）、待结算配额估算
（`gproxy:quota-pending:{window}`）、请求限流窗口
（`gproxy:rate:{limit}:{window_start}`）、凭证 RPM/TPM 窗口
（`gproxy:credential-rate:{credential}:{rpm|tpm}:{minute}`）、轮询视频任务的
结算去重、凭证刷新租约和会话亲和绑定。两个实例若各用进程内缓存，会各自独
立执行限流并各自刷新同一个 OAuth token，因此多实例部署需要 Redis、Upstash
或 libSQL 表。

控制面快照由做出更改的实例重建，随后该实例递增共享缓存中的
`gproxy:invalidate`。原生实例每秒轮询一次，Edge isolate 每次请求时检查；版本变化
时重新加载自己的快照。

## 备份

- SQLite：停止进程后复制 `<data-dir>/gproxy.db`，或使用 SQLite 在线备份
  （`sqlite3 gproxy.db ".backup gproxy-backup.db"`）。主密钥要与副本一起保
  存；没有 `GPROXY_MASTER_KEY`，密封的数据库无法读取。
- PostgreSQL 和 MySQL：使用数据库自带的转储工具。
- libSQL/Turso：使用平台的快照。
- 逻辑导出：控制台 → 设置 → 配置导入与导出（`POST /admin/api/export`、
  `POST /admin/api/import`）。导出包含 Provider、凭证、密钥、配额、定价、路
  由、别名和规则集。开启 `include_secrets` 时还包含凭证和密钥的秘密，以导
  出实例的密钥密封；导入用源主密钥打开它们并用本地密钥重新密封。用量、日
  志和审计行不导出；内置默认价格行也省略。

## 保留与大小压力

原生宿主每 5 分钟执行一次清理。它删除早于 `retention_days` 的
`usage_rows`、`request_logs` 和 `wire_logs`（每表每次 5,000 行），连同这些
用量行拥有的配额跟踪行和早于截止时间的配额活动记录，测量数据
库大小（SQLite 和 libSQL 用 `page_count × page_size`，PostgreSQL 用
`pg_database_size`，MySQL 用 `information_schema` 中的大小），当大小超过
`max_database_size_mb` 时删除最旧的 5,000 行 `request_logs` 和 `wire_logs`
——从不删除用量。未设置的值按 36,500 天和 1,024 MiB 处理。清理需要后台任务
调度器，因此在 Edge 上不运行；请用数据库供应商的工具约束 Edge 存储。

## Edge 宿主支持范围

wasm 宿主只编译 libSQL 后端以及 libSQL 和 Upstash 缓存。没有 SQLite 文件、
没有 PostgreSQL 或 MySQL 驱动、没有进程内缓存、没有 Redis 客户端、没有清理
任务，也没有 Hugging Face 词表注册表。
