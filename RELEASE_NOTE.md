# Release Notes

## v1.0.0

> **Breaking release.** gproxy v1.0.0 is a full ground-up rewrite of the v0.3.x
> line. Treat it as a brand-new project: the workspace layout, storage schema,
> HTTP API, admin surface, TOML config format, CLI flags, and provider settings
> have all changed and are **not** compatible with v0.3.42 or earlier. There is
> no in-place upgrade path — see the Compatibility section below.

### English

#### Added

- Brand-new workspace layout split into three layers:
  - `sdk/` — protocol & provider SDK: `gproxy-sdk`, `gproxy-protocol`,
    `gproxy-provider`, `gproxy-routing`. Handles protocol conversion, provider
    execution, credential health, routing and cache affinity.
  - `crates/` — app layer: `gproxy-core`, `gproxy-storage`, `gproxy-api`,
    `gproxy-server`. Owns HTTP routing, admin API, storage, and `AppState`.
  - `apps/` — binaries: `gproxy` (main server) and `gproxy-recorder`
    (standalone MITM recorder).
- New storage layer (`gproxy-storage`) built on SeaORM + SQLx with first-class
  support for **SQLite**, **PostgreSQL**, and **MySQL**. Schema is auto-synced
  on startup; DSN is passed via `GPROXY_DSN` or persisted in `global_settings`.
- New embedded browser console mounted at `/console` (built from
  `frontend/console`, shipped inside the binary via `rust-embed`). Browser
  login flow: `POST /login` → session API key → `Authorization: Bearer`.
- New admin HTTP API under `/admin/*` with query/upsert/delete/batch handlers
  for providers, credentials, credential statuses, models, model aliases,
  users, user keys, user permissions, file permissions, rate limits, quotas,
  global settings, request logs, usage logs, config export, and self-update.
- New user HTTP API under `/user/*` for self-service key management, quota
  lookup, and usage queries.
- New provider proxy surface with both **scoped** (`/{provider}/v1/...`) and
  **unscoped** (`/v1/...`) routes, covering Claude Messages, OpenAI Chat
  Completions, OpenAI Responses, Embeddings, Images, Models, Gemini v1beta,
  and provider file APIs (upload/list/get/delete/content).
- New WebSocket bridging: passthrough, OpenAI ↔ Gemini Live, and Gemini Live ↔
  OpenAI Responses, via `ws_bridge.rs`.
- Security hardening:
  - Password hashing uses **Argon2id** with explicit OWASP parameters
    (19 MiB memory, 2 iterations).
  - API keys are stored as **SHA-256 digests** with constant-time comparison
    to defeat timing attacks.
  - Optional **field-level encryption** for credentials / passwords / API keys
    via `DATABASE_SECRET_KEY` (XChaCha20Poly1305).
  - Admin API responses **mask credential secrets** so raw tokens never
    leak back out.
- Optional multi-instance backend via the `redis` Cargo feature. When the
  binary is built with `--features redis` and `GPROXY_REDIS_URL` is set,
  rate limiting, quota reservation, and cache affinity are dispatched to
  Redis (Lua-atomic INCR / reserve / settle) instead of the local process.
- New TOML seed-config format (`gproxy.example.toml` / `gproxy.example.full.toml`)
  driving first-time bootstrap via `[global]`, `[[providers]]`, `[[models]]`,
  `[[model_aliases]]`, `[[users]]`, `[[users.keys]]`, `[[permissions]]`,
  `[[file_permissions]]`, `[[rate_limits]]`, `[[quotas]]`.
- New standalone `gproxy-recorder` binary under `apps/gproxy-recorder` for
  capturing upstream LLM traffic independently of the main server.
- Graceful shutdown pipeline: Axum `with_graceful_shutdown`, a bounded worker
  set draining within 5s, `UsageSink` final flush, `HealthBroadcaster` debounce
  flush, and exit hooks for `QuotaReconciler` / `RateLimitGC`.

#### Changed

- Workspace version bumped from `0.3.42` to **`1.0.0`**.
- Workspace members rewritten from a flat `crates/` list (`gproxy-admin`,
  `gproxy-middleware`, `gproxy-protocol`, `gproxy-provider`, `gproxy-core`,
  `gproxy-storage`) to the new `sdk/ + crates/ + apps/` layout described above.
  The old `gproxy-admin` and `gproxy-middleware` crates no longer exist.
- All provider execution now goes through `gproxy-sdk`'s `GproxyEngine`.
  Provider registration, credential dispatch, protocol conversion, and cache
  affinity are owned by the SDK; the app layer only orchestrates HTTP,
  storage, and `AppState`.
- Admin mutations follow an explicit **DB-first** model: write storage →
  sync `AppState` → rebuild `GproxyEngine` atomically via `ArcSwap`. Hot
  reload is exposed through `POST /admin/reload`.
- Read paths are **memory-first**: auth, permission checks, rate limiting,
  quota checks and alias resolution all run out of `AppState` `ArcSwap` /
  `DashMap` structures. The database is no longer on the request hot path.
- Bootstrap precedence: existing DB → TOML seed (`GPROXY_CONFIG`) →
  built-in defaults. Seed is persisted before being loaded into memory so
  `reload_from_db()` fully reconstructs state after a crash.
- CLI / environment variables have been reworked around the new app. The
  full set lives in `apps/gproxy/src/main.rs` and includes `GPROXY_HOST`,
  `GPROXY_PORT`, `GPROXY_ADMIN_USER`, `GPROXY_ADMIN_PASSWORD`,
  `GPROXY_ADMIN_API_KEY`, `GPROXY_DSN`, `GPROXY_CONFIG`, `GPROXY_DATA_DIR`,
  `GPROXY_PROXY`, `GPROXY_SPOOF`, `DATABASE_SECRET_KEY`, and (with the
  `redis` feature) `GPROXY_REDIS_URL`.
- Credential health is now managed by the SDK at runtime and snapshotted
  into a dedicated `credential_statuses` table; bootstrap restores the
  snapshot back into SDK memory.
- Release validation script now runs workspace-wide `cargo fmt` +
  `cargo clippy --workspace --all-targets -- -D warnings
  -A clippy::too_many_arguments` before tagging.

#### Removed

- The entire v0.3.x admin UI, provider settings schema, and channel-specific
  toggles. Legacy fields like `claudecode_enable_billing_header`,
  `claudecode_flatten_system_text_before_cache_control`,
  `enable_claude_1m_sonnet` / `enable_claude_1m_opus`, `priority_tier`,
  `claudecode_extra_beta_headers`, etc. are **not** carried over. Any of
  these behaviors you need to retain must be re-expressed against the new
  v1 provider/credential schema.
- Legacy v0.3.x storage tables, write-event variants, and on-disk layout.
  There is no automated migration from v0.3.x SQLite / MySQL / Postgres
  databases to the v1 schema.
- Old `crates/gproxy-admin` and `crates/gproxy-middleware` crates. Their
  responsibilities are now split across `gproxy-api`, `gproxy-server`, and
  the `sdk/` crates.
- Legacy per-channel credential status semantics — the new `gproxy-sdk`
  classifies failures (transient vs dead vs cooldown) uniformly across
  providers, so previously tuned per-channel fallbacks (e.g. Claude 1M
  automatic retry, Codex `402 deactivated_workspace` handling) are replaced
  by the SDK's unified health model.
- Old `release.sh` assumptions about a flat `crates/` layout. See the new
  script at `release.sh`.

#### Compatibility

- **This is a hard break from v0.3.x.** No automated migration path is
  provided. Plan to stand up a fresh database, regenerate admin and user
  credentials, and re-enter providers / models / aliases / permissions /
  quotas against the new v1 schema.
- Old `gproxy.toml` files from v0.3.x will not load as-is. Rewrite them
  against `gproxy.example.toml` / `gproxy.example.full.toml` before
  starting v1.
- HTTP clients that previously called v0.3.x admin routes must be updated
  to the new `/admin/*` query/upsert/delete/batch surface; request and
  response payloads have changed.
- User-facing provider proxy routes (`/v1/...`, `/{provider}/v1/...`) are
  compatible at the protocol level with standard Claude / OpenAI / Gemini
  clients, but auth, model aliasing, and permission errors are returned
  through the v1 error shape — downstream integrations that parsed v0.3.x
  error payloads should be reviewed.
- Credential secrets, passwords, and API keys should be re-imported after
  `DATABASE_SECRET_KEY` has been decided. Switching `DATABASE_SECRET_KEY`
  later is not a supported in-place operation.
- Multi-instance deployments that relied on the v0.3.x in-process counters
  must now opt into the `redis` feature and point `GPROXY_REDIS_URL` at a
  shared Redis instance; otherwise rate limit and quota state remain
  per-process.
- `gproxy-recorder` is independent from the main binary. If you previously
  used an ad-hoc recording setup in v0.3.x, switch to the new recorder app.

### 中文

#### 新增

- 全新的三层 workspace 布局：
  - `sdk/` — 协议与 provider SDK：`gproxy-sdk`、`gproxy-protocol`、
    `gproxy-provider`、`gproxy-routing`。负责协议转换、provider 执行、凭证
    健康、路由与缓存亲和。
  - `crates/` — 应用层：`gproxy-core`、`gproxy-storage`、`gproxy-api`、
    `gproxy-server`。负责 HTTP 路由、admin API、存储与 `AppState`。
  - `apps/` — 可执行程序：`gproxy`（主服务）与 `gproxy-recorder`
    （独立的 MITM 录制工具）。
- 全新的存储层（`gproxy-storage`），基于 SeaORM + SQLx，原生支持
  **SQLite**、**PostgreSQL**、**MySQL**。启动时会自动同步 schema；DSN 可
  通过 `GPROXY_DSN` 传入，或从 `global_settings` 中恢复。
- 全新的嵌入式浏览器控制台，挂载在 `/console`（由 `frontend/console`
  构建，通过 `rust-embed` 打入二进制）。浏览器登录流程：`POST /login` →
  会话 API key → `Authorization: Bearer`。
- 全新的 admin API：`/admin/*` 下统一提供 providers、credentials、
  credential statuses、models、model aliases、users、user keys、用户权限、
  文件权限、限流、配额、全局设置、请求日志、用量日志、配置导出与自更新的
  query / upsert / delete / batch 接口。
- 全新的 user API：`/user/*`，供用户自助管理 API key、查询配额与用量。
- 全新的 provider 代理入口，同时提供 **scoped**（`/{provider}/v1/...`）
  与 **unscoped**（`/v1/...`）两种路径，覆盖 Claude Messages、OpenAI Chat
  Completions、OpenAI Responses、Embeddings、Images、Models、Gemini
  v1beta，以及 provider 文件 API（上传/列出/查询/删除/下载）。
- 全新的 WebSocket 桥接：同协议透传、OpenAI ↔ Gemini Live、Gemini Live ↔
  OpenAI Responses，统一由 `ws_bridge.rs` 实现。
- 安全加固：
  - 用户密码统一使用 **Argon2id**，并按 OWASP 建议固定参数
    （19 MiB 内存、2 次迭代）。
  - API key 以 **SHA-256 摘要** 方式存储，使用常量时间比对，防御计时
    攻击。
  - 可选的字段级加密，通过 `DATABASE_SECRET_KEY`
    （XChaCha20Poly1305）加密凭证 / 密码 / API key。
  - Admin API 返回时会 **脱敏凭证密钥**，上游 token 不会再原样回吐。
- 可选的多实例后端：`redis` Cargo feature。当启用 `--features redis`
  并设置 `GPROXY_REDIS_URL` 时，限流、配额预留和缓存亲和会通过 Redis
  Lua 原子脚本完成，不再依赖单进程本地状态。
- 全新的 TOML 种子配置（`gproxy.example.toml` / `gproxy.example.full.toml`），
  用于首次启动时初始化 DB：`[global]`、`[[providers]]`、`[[models]]`、
  `[[model_aliases]]`、`[[users]]`、`[[users.keys]]`、`[[permissions]]`、
  `[[file_permissions]]`、`[[rate_limits]]`、`[[quotas]]`。
- 独立的 `gproxy-recorder` 可执行程序（`apps/gproxy-recorder`），用于
  脱离主服务独立抓取上游 LLM 流量。
- 优雅关闭流水线：Axum `with_graceful_shutdown`、5 秒封顶的 worker
  收敛、`UsageSink` 终态刷写、`HealthBroadcaster` 去抖 flush，以及
  `QuotaReconciler` / `RateLimitGC` 的退出钩子。

#### 变更

- workspace 版本由 `0.3.42` 升级到 **`1.0.0`**。
- workspace 成员从旧的扁平 `crates/` 列表（`gproxy-admin`、
  `gproxy-middleware`、`gproxy-protocol`、`gproxy-provider`、`gproxy-core`、
  `gproxy-storage`）重构为上述 `sdk/ + crates/ + apps/` 三层布局。
  原有的 `gproxy-admin`、`gproxy-middleware` crate 已经不再存在。
- 所有 provider 执行现在都通过 `gproxy-sdk` 的 `GproxyEngine`。provider
  注册、凭证调度、协议转换与缓存亲和由 SDK 掌握；app 层只负责编排 HTTP、
  存储与 `AppState`。
- Admin mutation 遵循明确的 **DB-first** 模型：先写存储 → 同步 `AppState`
  → 通过 `ArcSwap` 原子替换 `GproxyEngine`。热重载通过
  `POST /admin/reload` 暴露。
- 读路径为 **Memory-first**：鉴权、权限、限流、配额检查、别名解析等全部
  走 `AppState` 的 `ArcSwap` / `DashMap`。数据库不再出现在请求热路径上。
- Bootstrap 优先级：已有 DB → TOML 种子（`GPROXY_CONFIG`）→ 内置默认。
  种子会先落 DB 再加载到内存，保证崩溃后 `reload_from_db()` 能够完整恢复。
- CLI / 环境变量围绕新应用重新梳理。完整列表见
  `apps/gproxy/src/main.rs`，包括 `GPROXY_HOST`、`GPROXY_PORT`、
  `GPROXY_ADMIN_USER`、`GPROXY_ADMIN_PASSWORD`、`GPROXY_ADMIN_API_KEY`、
  `GPROXY_DSN`、`GPROXY_CONFIG`、`GPROXY_DATA_DIR`、`GPROXY_PROXY`、
  `GPROXY_SPOOF`、`DATABASE_SECRET_KEY`，以及启用 `redis` feature 后的
  `GPROXY_REDIS_URL`。
- 凭证健康状态现在由 SDK 在运行时维护，并定期快照到 `credential_statuses`
  表；bootstrap 时再从快照恢复回 SDK 内存。
- 发版校验脚本现在会在打 tag 前运行 workspace 级 `cargo fmt` +
  `cargo clippy --workspace --all-targets -- -D warnings
  -A clippy::too_many_arguments`。

#### 移除

- 整套 v0.3.x 的后台 UI、provider 设置结构与渠道专用开关。旧字段如
  `claudecode_enable_billing_header`、
  `claudecode_flatten_system_text_before_cache_control`、
  `enable_claude_1m_sonnet` / `enable_claude_1m_opus`、`priority_tier`、
  `claudecode_extra_beta_headers` 等 **均未保留**。如果需要这些行为，请
  按 v1 新的 provider / credential schema 重新表达。
- v0.3.x 的存储表结构、write-event 变体以及落盘布局。不提供从 v0.3.x
  SQLite / MySQL / Postgres 到 v1 schema 的自动迁移。
- 旧的 `crates/gproxy-admin`、`crates/gproxy-middleware` crate。其职责
  已拆分到 `gproxy-api`、`gproxy-server` 及 `sdk/` 下。
- 老版本按渠道定制的凭证健康语义——新的 `gproxy-sdk` 会跨 provider 统一
  分类失败（瞬时 / dead / cooldown），因此之前针对特定渠道的降级逻辑
  （如 Claude 1M 自动重试、Codex `402 deactivated_workspace` 专用处理）
  均被统一的 SDK 健康模型取代。
- 原先假设扁平 `crates/` 布局的 `release.sh`。请使用仓库根目录下的新
  `release.sh`。

#### 兼容性

- **这是相对 v0.3.x 的硬断代。** 不提供任何自动迁移路径。请按全新项目
  对待：新建数据库，重新生成 admin / user 凭证，并在 v1 schema 下重新
  配置 providers / models / aliases / permissions / quotas。
- v0.3.x 时代的 `gproxy.toml` 无法直接加载。请参照
  `gproxy.example.toml` / `gproxy.example.full.toml` 重新编写后再启动 v1。
- 依赖 v0.3.x admin 路由的 HTTP 客户端必须全面迁移到新的 `/admin/*`
  query / upsert / delete / batch 接口；请求与响应 payload 均已变更。
- 面向用户的 provider 代理路由（`/v1/...`、`/{provider}/v1/...`）在协议
  层仍兼容标准 Claude / OpenAI / Gemini 客户端；但鉴权、模型别名、权限
  等错误会按 v1 的错误结构返回，下游如果此前解析过 v0.3.x 错误体，请
  重新核对。
- 凭证密钥、用户密码、API key 应在确定 `DATABASE_SECRET_KEY` 之后再
  重新导入。运行后再切换 `DATABASE_SECRET_KEY` 不是受支持的原地操作。
- 依赖 v0.3.x 进程内限流 / 配额计数的多实例部署，必须启用 `redis`
  feature 并把 `GPROXY_REDIS_URL` 指向共享 Redis；否则限流与配额仍然
  是进程级的。
- `gproxy-recorder` 独立于主服务。如果此前在 v0.3.x 使用了临时的录制
  方案，请迁移到新的 recorder 二进制。
