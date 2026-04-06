# App 层总览 / App Layer Overview

[中文](#中文) | [English](#english)

---

## 中文

### 架构概述

gproxy 的 App 层可以按下面这条链路理解：

| 层级 / Layer | crate / 目录 / Crate or Directory | 主要职责 / Primary Responsibility |
| --- | --- | --- |
| 领域层 / Domain layer | `crates/gproxy-core` | 提供内存态 Domain Service，包括身份、策略、配额、路由、文件、配置，以及可选的 Redis backend。 / Provides in-memory domain services for identity, policy, quota, routing, files, configuration, and an optional Redis backend. |
| 持久化层 / Persistence layer | `crates/gproxy-storage` | 基于 SeaORM 管理数据库连接、Schema 同步、查询仓储和写入事件。 / Manages database connections, schema sync, query repositories, and write events through SeaORM. |
| API 层 / API layer | `crates/gproxy-api` | 用 Axum 组织登录、Admin、User、Provider HTTP / WebSocket 路由，并负责 bootstrap。 / Organizes login, Admin, User, and Provider HTTP/WebSocket routes with Axum and handles bootstrap. |
| 应用入口 / Application entry | `apps/gproxy` | 解析 CLI / 环境变量、连接数据库、创建 `AppState`、启动后台 worker 与 HTTP Server。 / Parses CLI arguments and environment variables, connects to the database, creates `AppState`, and starts background workers plus the HTTP server. |

运行时真正把这些层装配在一起的是 `crates/gproxy-server` 里的 `AppState` / `AppStateBuilder`。`apps/gproxy/src/main.rs` 先创建 `GlobalConfig`、`SeaOrmStorage`、SDK engine 和 worker，再把 `gproxy-core` 的服务组合进共享状态，最后交给 `gproxy-api::api_router` 暴露接口。

### 6 个 Domain Service

下表列出六个核心内存态 service：

| Service | 源码 / Source | 作用 / Responsibility |
| --- | --- | --- |
| Identity | `crates/gproxy-core/src/identity.rs` | 维护用户与 API Key 内存快照；API Key 先做带域分隔符的 SHA-256 摘要，再作为 HashMap 键，避免直接用明文 key 做查找；负责认证、按用户查询 key，以及用户 / key 的原子替换与单条 CRUD。 / Maintains in-memory snapshots of users and API keys; API keys are first hashed with SHA-256 plus a domain separator before becoming HashMap keys, avoiding plaintext lookups; handles authentication, per-user key lookup, atomic replacement, and single-item CRUD for users and keys. |
| Policy | `crates/gproxy-core/src/policy.rs` | 维护用户模型权限、文件权限和限流规则；负责模型访问判断、Provider 访问判断、文件上传权限判断，以及按模型模式查找限流规则。 / Maintains user model permissions, file permissions, and rate-limit rules; checks model access, provider access, file-upload permissions, and rate-limit rules by model pattern. |
| Quota | `crates/gproxy-core/src/quota.rs` | 用 `DashMap` 维护每个用户的 `(quota_total, cost_used)`；支持配额检查、累计成本、整表替换和快照导出。 / Uses `DashMap` to maintain `(quota_total, cost_used)` for each user; supports quota checks, cost accumulation, full replacement, and snapshot export. |
| Routing | `crates/gproxy-core/src/routing.rs` | 维护模型表、模型别名、`provider_name -> provider_id/channel` 映射、Provider 到凭证 ID 列表的索引；负责别名解析、模型查找、凭证定位。 / Maintains model catalogs, model aliases, `provider_name -> provider_id/channel` mappings, and indexes from providers to credential ID lists; handles alias resolution, model lookup, and credential lookup. |
| File | `crates/gproxy-core/src/file.rs` | 维护用户文件记录和 Claude 文件元数据缓存；支持按用户 / Provider / file_id 查找活动文件，以及批量替换与单条更新。 / Maintains user file records and cached Claude file metadata; supports looking up active files by user, provider, and `file_id`, as well as batch replacement and single-item updates. |
| Config | `crates/gproxy-core/src/config.rs` | 用 `ArcSwap<GlobalConfig>` 持有当前全局配置，提供原子读取与替换。 / Holds the current global configuration with `ArcSwap<GlobalConfig>` and provides atomic reads and swaps. |

这些 Service 都是“内存态真相”的持有者。数据库里的持久化数据由 `gproxy-storage` 提供，启动或 `/admin/reload` 时重新装载到这些 Service 中；请求路径上的鉴权、权限判断、配额与路由解析则直接读取这些内存服务。

### Background Workers

| Worker | 源码 / Source | 触发方式 / Trigger | 作用 / Responsibility |
| --- | --- | --- | --- |
| UsageSink | `apps/gproxy/src/workers/usage_sink.rs` | mpsc 队列，满 100 条或 500ms flush。 / mpsc queue, flushing at 100 items or every 500 ms. | 异步批量写 usage 记录，避免数据面请求阻塞数据库写入；停机时会关闭接收端、排空剩余消息并做最后一次 flush。 / Writes usage records asynchronously in batches so data-plane requests do not block on database writes; on shutdown it closes the receiver, drains remaining messages, and performs one final flush. |
| QuotaReconciler | `apps/gproxy/src/workers/quota_reconciler.rs` | 每 30 秒轮询。 / Polls every 30 seconds. | 从数据库读取配额真相，修正本地 `QuotaService`；主要处理管理端改配额、跨实例成本增加等场景。 / Reads the source-of-truth quota state from the database and repairs the local `QuotaService`, mainly for admin-side quota changes or cross-instance cost growth. |
| HealthBroadcaster | `apps/gproxy/src/workers/health_broadcaster.rs` | 订阅 SDK `EngineEvent`，500ms 防抖。 / Subscribes to SDK `EngineEvent` with a 500 ms debounce window. | 监听凭证健康状态变化，把 `(provider, credential index)` 解析成数据库凭证 ID，然后持久化到 `credential_statuses`。 / Watches credential health-state changes, resolves `(provider, credential index)` into database credential IDs, and persists them into `credential_statuses`. |
| RateLimitGC | `apps/gproxy/src/workers/rate_limit_gc.rs` | 每 60 秒轮询。 / Polls every 60 seconds. | 清理内存态 rate-limit counter 中已过期的窗口计数。 / Cleans expired window counters from the in-memory rate-limit state. |

所有 worker 都通过 `WorkerSet` 共享一个 `watch<bool>` 关闭信号。应用退出时会发送 shutdown，并最多等待 5 秒让 worker 排空缓冲区；超时会记录 warning。

### 启动流程

源码里的启动顺序可以概括为：

`CLI 参数 -> DB 连接 -> Bootstrap -> Workers -> HTTP Server`

对应到 `apps/gproxy/src/main.rs` 的实际细节如下：

1. 初始化 tracing，默认日志级别为 `info`，也支持 `RUST_LOG` 覆盖。
2. 解析 CLI / 环境变量，得到 `host`、`port`、`dsn`、`config`、`data_dir`、`proxy`、`spoof`、`admin_key`、`DATABASE_SECRET_KEY`、`GPROXY_REDIS_URL`。
3. 解析数据库 DSN。默认 DSN 由 `data_dir/gproxy.db` 生成，格式为 `sqlite://<data_dir>/gproxy.db?mode=rwc`。
4. 创建数据目录，连接数据库并执行 `storage.sync()` 做 schema 同步。
5. 若 CLI 没有显式传入 `dsn` / `data_dir`，且数据库中的 `global_settings` 持久化了另一套 DSN，则启动阶段会按持久化设置重新连接数据库。
6. 构造 `GlobalConfig`，再按需连接 Redis。只有编译了 `redis` feature 且设置 `GPROXY_REDIS_URL` 时，才会初始化 Redis backend。
7. 先启动 `UsageSink`。这是唯一一个在 `AppState` 创建前就启动的 worker，因为 `AppStateBuilder` 需要拿到 `usage_tx`。
8. 构造 SDK engine 与 `AppStateBuilder`，注入 `storage`、`config`、`usage_tx`，如果可用则注入 Redis quota / rate-limit backend。
9. 执行 bootstrap。如果数据库已有 `global_settings`，调用 `reload_from_db` 从数据库恢复完整内存状态；否则如果 `GPROXY_CONFIG` 指向的 TOML 文件存在，则按 TOML 初始化；再否则只写入最小默认配置。
10. 把启动阶段显式传入的 host、port、proxy、spoof、dsn、data_dir 和 admin key 回写到全局配置。首次启动且没有现成 admin key 时，会生成一个 UUID v7 作为管理员 key，并持久化到数据库。
11. 启动剩余 worker：`QuotaReconciler`、`RateLimitGC`、`HealthBroadcaster`。
12. 构造 `gproxy_api::api_router(state)`，绑定 `host:port`，启动 Axum HTTP Server。
13. 收到 `Ctrl+C` 或 `SIGTERM` 后进入优雅停机：先让 HTTP Server 停止服务，再通知 worker 关闭并等待排空。

### 运行时协作关系

- `gproxy-api` 的鉴权和路由中间件直接读 `AppState` 里的 `IdentityService`、`PolicyService`、`RoutingService` 和 `ConfigService`。
- `gproxy-storage` 是持久化真相源，`reload_from_db` / `seed_from_toml` 把数据库或 TOML 转换成 `gproxy-core` 的内存模型。
- `QuotaService`、rate-limit counter 和文件缓存是请求路径上的热数据；其中 quota 还会被 `QuotaReconciler` 周期性修正。
- Provider 相关的实际上游转发能力来自 SDK engine，但 Provider 名称、别名、权限、凭证索引都由 App 层状态决定。

---

## English

### Architecture Overview

The gproxy application layer can be understood through the pipeline above. The shared bilingual table describes the domain, persistence, API, and entry layers, while `AppState` and `AppStateBuilder` in `crates/gproxy-server` are what actually wire them together at runtime. In `apps/gproxy/src/main.rs`, the process first creates `GlobalConfig`, `SeaOrmStorage`, the SDK engine, and the workers, then combines the `gproxy-core` services into shared state and finally exposes the API through `gproxy_api::api_router`.

### The Six Domain Services

See the shared bilingual table above for the six in-memory services and the exact responsibility of each one. As in the Chinese section, these services are the holders of the in-memory source of truth, while `gproxy-storage` provides the persisted state that is reloaded into them during startup or `/admin/reload`.

### Background Workers

See the shared bilingual table above for the current worker set, how each worker is triggered, and what it persists or reconciles. All workers share a `watch<bool>` shutdown signal through `WorkerSet`, and the process waits up to five seconds for buffers to drain during shutdown before logging a warning.

### Startup Flow

The startup order in the source can be summarized as:

`CLI arguments -> DB connection -> Bootstrap -> Workers -> HTTP Server`

The 13-step Chinese section above gives the exact runtime sequence from tracing initialization and DSN resolution through bootstrap, worker startup, Axum server startup, and graceful shutdown wiring.

### Runtime Collaboration

- `gproxy-api` authentication and routing middleware read `IdentityService`, `PolicyService`, `RoutingService`, and `ConfigService` directly from `AppState`.
- `gproxy-storage` is the persisted source of truth, and `reload_from_db` / `seed_from_toml` translate database or TOML state into the in-memory models owned by `gproxy-core`.
- `QuotaService`, the rate-limit counters, and the file cache are hot-path request data, and quota is additionally repaired periodically by `QuotaReconciler`.
- Actual upstream forwarding comes from the SDK engine, but provider names, aliases, permissions, and credential indexes are owned by the application-layer state.
