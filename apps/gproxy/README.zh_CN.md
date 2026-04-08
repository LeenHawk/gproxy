# App 层总览

## 架构概述

gproxy 的 App 层可以按下面这条链路理解：

| 层级 | crate / 目录 | 主要职责 |
| --- | --- | --- |
| 领域层 | `crates/gproxy-core` | 提供内存态 Domain Service，包括身份、策略、配额、路由、文件、配置，以及可选的 Redis backend。 |
| 持久化层 | `crates/gproxy-storage` | 基于 SeaORM 管理数据库连接、Schema 同步、查询仓储和写入事件。 |
| API 层 | `crates/gproxy-api` | 用 Axum 组织登录、Admin、User、Provider HTTP / WebSocket 路由，并负责 bootstrap。 |
| 应用入口 | `apps/gproxy` | 解析 CLI / 环境变量、连接数据库、创建 `AppState`、启动后台 worker，并同时服务 API 路由和内嵌的 `/console` 前端。 |

运行时真正把这些层装配在一起的是 `crates/gproxy-server` 里的 `AppState` / `AppStateBuilder`。`apps/gproxy/src/main.rs` 先创建 `GlobalConfig`、`SeaOrmStorage`、SDK engine 和 worker，再把 `gproxy-core` 的服务组合进共享状态，最后交给 `gproxy-api::api_router` 暴露接口。

## 6 个 Domain Service

下表列出六个核心内存态 service：

| Service | 源码 | 作用 |
| --- | --- | --- |
| Identity | `crates/gproxy-core/src/identity.rs` | 维护用户与 API Key 内存快照；API Key 先做带域分隔符的 SHA-256 摘要，再作为 HashMap 键，避免直接用明文 key 做查找；负责认证、按用户查询 key，以及用户 / key 的原子替换与单条 CRUD。 |
| Policy | `crates/gproxy-core/src/policy.rs` | 维护用户模型权限、文件权限和限流规则；负责模型访问判断、Provider 访问判断、文件上传权限判断，以及按模型模式查找限流规则。 |
| Quota | `crates/gproxy-core/src/quota.rs` | 用 `DashMap` 维护每个用户的 `(quota_total, cost_used)`；支持配额检查、累计成本、整表替换和快照导出。 |
| Routing | `crates/gproxy-core/src/routing.rs` | 维护模型表、模型别名、`provider_name -> provider_id/channel` 映射、Provider 到凭证 ID 列表的索引；负责别名解析、模型查找、凭证定位。 |
| File | `crates/gproxy-core/src/file.rs` | 维护用户文件记录和 Claude 文件元数据缓存；支持按用户 / Provider / file_id 查找活动文件，以及批量替换与单条更新。 |
| Config | `crates/gproxy-core/src/config.rs` | 用 `ArcSwap<GlobalConfig>` 持有当前全局配置，提供原子读取与替换。 |

这些 Service 都是"内存态真相"的持有者。数据库里的持久化数据由 `gproxy-storage` 提供，启动或 `/admin/reload` 时重新装载到这些 Service 中；请求路径上的鉴权、权限判断、配额与路由解析则直接读取这些内存服务。

## Background Workers

| Worker | 源码 | 触发方式 | 作用 |
| --- | --- | --- | --- |
| UsageSink | `apps/gproxy/src/workers/usage_sink.rs` | mpsc 队列，满 100 条或 500ms flush。 | 异步批量写 usage 记录，避免数据面请求阻塞数据库写入；停机时会关闭接收端、排空剩余消息并做最后一次 flush。 |
| QuotaReconciler | `apps/gproxy/src/workers/quota_reconciler.rs` | 每 30 秒轮询。 | 从数据库读取配额真相，修正本地 `QuotaService`；主要处理管理端改配额、跨实例成本增加等场景。 |
| HealthBroadcaster | `apps/gproxy/src/workers/health_broadcaster.rs` | 订阅 SDK `EngineEvent`，500ms 防抖。 | 监听凭证健康状态变化，把 `(provider, credential index)` 解析成数据库凭证 ID，然后持久化到 `credential_statuses`。 |
| RateLimitGC | `apps/gproxy/src/workers/rate_limit_gc.rs` | 每 60 秒轮询。 | 清理内存态 rate-limit counter 中已过期的窗口计数。 |

所有 worker 都通过 `WorkerSet` 共享一个 `watch<bool>` 关闭信号。应用退出时会发送 shutdown，并最多等待 5 秒让 worker 排空缓冲区；超时会记录 warning。

## 启动流程

源码里的启动顺序可以概括为：

`CLI 参数 -> DB 连接 -> Bootstrap -> Workers -> HTTP Server`

对应到 `apps/gproxy/src/main.rs` 的实际细节如下：

1. 初始化 tracing，默认日志级别为 `info`，也支持 `RUST_LOG` 覆盖。
2. 解析 CLI / 环境变量，得到 `host`、`port`、`dsn`、`config`、`data_dir`、`proxy`、`spoof`、bootstrap admin 用户名 / 密码 / API key，以及 `DATABASE_SECRET_KEY`。
3. 解析数据库 DSN。默认 DSN 由 `data_dir/gproxy.db` 生成，格式为 `sqlite://<data_dir>/gproxy.db?mode=rwc`。
4. 创建数据目录，连接数据库并执行 `storage.sync()` 做 schema 同步。
5. 若 CLI 没有显式传入 `dsn` / `data_dir`，且数据库中的 `global_settings` 持久化了另一套 DSN，则启动阶段会按持久化设置重新连接数据库。
6. 构造 `GlobalConfig`，再按需连接 Redis。只有编译了 `redis` feature 且设置 `GPROXY_REDIS_URL` 时，才会初始化 Redis backend。
7. 先创建 `UsageSink` channel，把 sender 注入 `AppStateBuilder`；真正的 worker 在 `AppState` 构造完成后启动，这样它可以始终读取最新的 `storage`。
8. 构造 SDK engine 与 `AppStateBuilder`，注入 `storage`、`config`、`usage_tx`。
9. 执行 bootstrap。如果数据库已有 `global_settings`，调用 `reload_from_db` 从数据库恢复完整内存状态；否则如果 `GPROXY_CONFIG` 指向的 TOML 文件存在，则按 TOML 初始化；再否则创建最小运行时配置、一个真实的 admin 用户和一把 bootstrap admin API key。
10. 把启动阶段显式传入的 host、port、proxy、spoof、dsn、data_dir 回写到全局配置。管理员身份不再存放在 `global_settings`，而是存放在 `users.is_admin` 和对应的 user key 中；首次启动缺少 bootstrap 密码或 API key 时，会生成并打印一次。
11. 启动剩余 worker：`QuotaReconciler`、`RateLimitGC`、`HealthBroadcaster`。
12. 构造 `gproxy_api::api_router(state)`，再合并 `apps/gproxy/src/web.rs` 提供的 `/console` 内嵌前端路由，绑定 `host:port`，启动 Axum HTTP Server。
13. 收到 `Ctrl+C` 或 `SIGTERM` 后进入优雅停机：先让 HTTP Server 停止服务，再通知 worker 关闭并等待排空。

## 内嵌控制台工作流

内嵌控制台资源位于 `apps/gproxy/web/console/`，由 `apps/gproxy/src/web.rs` 对外服务。

当前端变更后：

```bash
cd frontend/console
pnpm install
pnpm build
```

这会构建 SPA 并把产物同步到 embed 目录。之后再执行 `cargo run -p gproxy` 或 `cargo build -p gproxy`。

运行时：

- 打开 `/console`
- 通过 `/login` 登录
- 浏览器后续访问 `/admin/*` 和 `/user/*` 时使用 session token

## 运行时协作关系

- `gproxy-api` 的鉴权和路由中间件直接读 `AppState` 里的 `IdentityService`、`PolicyService`、`RoutingService` 和 `ConfigService`。
- `gproxy-storage` 是持久化真相源，`reload_from_db` / `seed_from_toml` 把数据库或 TOML 转换成 `gproxy-core` 的内存模型。
- `QuotaService`、rate-limit counter 和文件缓存是请求路径上的热数据；其中 quota 还会被 `QuotaReconciler` 周期性修正。
- Provider 相关的实际上游转发能力来自 SDK engine，但 Provider 名称、别名、权限、凭证索引都由 App 层状态决定。
