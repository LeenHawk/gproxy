# Release Notes

## v1.0.3

### English

#### Added

- **Suffix system for model-list / model-get** — suffix modifiers (e.g. `-thinking-high`, `-fast`) are now expanded in model list responses and rewritten in model get responses, so clients can discover available suffix variants.
- **Suffix per-channel toggle** — new `enable_suffix` setting lets operators enable/disable suffix processing per channel.
- **VertexExpress local model catalogue** — model list/get requests are served from a static model catalogue embedded at compile time, since Vertex AI Express does not expose a standard model-listing endpoint.
- **Vertex SA token bootstrap on credential upsert** — when a Vertex credential with `client_email` and `private_key` is added via the admin API, the access token is automatically obtained so the first request has valid auth.

#### Fixed

- **GeminiCLI / Antigravity model list** — both channels now correctly route model list/get through their respective quota/model endpoints (`retrieveUserQuota` for GeminiCLI, `fetchAvailableModels` for Antigravity) and normalize responses to standard Gemini format.
- **Vertex model list normalization** — Vertex AI returns `publisherModels` with full resource paths; responses are now converted to standard Gemini `models` format.
- **Vertex / VertexExpress header filtering** — `anthropic-version` and `anthropic-beta` headers are dropped before forwarding to Google endpoints.
- **Vertex GeminiCLI-style User-Agent** — Vertex requests now send proper `User-Agent` and `x-goog-api-client` headers matching Gemini CLI traffic.
- **Engine HTTP client proxy** — database proxy settings now take effect after bootstrap; previously the engine client was built before DB config was loaded.
- **Engine HTTP/1.1 for standard client** — the non-spoof wreq client uses `http1_only()` for reliable proxy traversal.
- **HTTP client request dispatch** — switched from `wreq::Request::from() + execute()` to `client.request().send()` to ensure proxy/TLS settings propagate correctly.
- **Frontend: VertexExpress credential** — field changed from `access_token` to `api_key`.
- **Frontend: Vertex credential** — added missing optional fields (`private_key_id`, `client_id`, `token_uri`).

---

### 中文

#### 新增

- **Suffix 系统支持 model-list / model-get** — suffix 修饰符（如 `-thinking-high`、`-fast`）现在会在模型列表响应中展开、在模型详情响应中回写，客户端可以发现可用的 suffix 变体。
- **Suffix 按渠道开关** — 新增 `enable_suffix` 配置项，可按渠道启用/禁用 suffix 处理。
- **VertexExpress 本地模型目录** — model list/get 请求从编译时嵌入的静态模型目录返回，因为 Vertex AI Express 没有标准的模型列表端点。
- **Vertex SA 凭证 upsert 自动换 token** — 通过 admin API 添加包含 `client_email` 和 `private_key` 的 Vertex 凭证时，自动获取 access token，首次请求不会因空 token 失败。

#### 修复

- **GeminiCLI / Antigravity 模型列表** — 两个渠道现在正确通过各自的配额/模型端点（GeminiCLI 用 `retrieveUserQuota`，Antigravity 用 `fetchAvailableModels`）路由 model list/get 请求，并将响应整形为标准 Gemini 格式。
- **Vertex 模型列表整形** — Vertex AI 返回的 `publisherModels`（含完整资源路径）现在被转换为标准 Gemini `models` 格式。
- **Vertex / VertexExpress 头过滤** — 转发到 Google 端点前丢弃 `anthropic-version` 和 `anthropic-beta` 头。
- **Vertex GeminiCLI 风格 User-Agent** — Vertex 请求现在发送匹配 Gemini CLI 流量的 `User-Agent` 和 `x-goog-api-client` 头。
- **Engine HTTP 客户端代理** — 数据库代理设置现在在自举后生效；之前 engine 客户端在 DB 配置加载前就已构建。
- **Engine 标准客户端 HTTP/1.1** — 非伪装 wreq 客户端使用 `http1_only()` 确保代理穿透可靠。
- **HTTP 客户端请求调度** — 从 `wreq::Request::from() + execute()` 改为 `client.request().send()`，确保代理/TLS 设置正确传递。
- **前端：VertexExpress 凭证** — 字段从 `access_token` 改为 `api_key`。
- **前端：Vertex 凭证** — 添加缺失的可选字段（`private_key_id`、`client_id`、`token_uri`）。

## v1.0.2

### English

#### Added

- **WebSocket per-model usage tracking** — when the client switches models mid-session (e.g. via `response.create`), usage is segmented per model and recorded separately instead of attributing all tokens to the last model.
- **WebSocket upstream message logging** — WS session end now records an upstream request log containing all client→server and server→client messages as request/response body.

---

### 中文

#### 新增

- **WebSocket 按模型分段用量** — 客户端在 WS 会话中切换模型时，用量按模型分段记录，不再把所有 token 归到最后一个模型。
- **WebSocket 上游消息日志** — WS session 结束时记录上游请求日志，包含所有客户端→服务器和服务器→客户端消息。

## v1.0.1

### English

#### Added

- **Upstream request logging** — quota queries and cookie exchange HTTP steps
  are now recorded in the `upstream_requests` table, giving full visibility
  into every outbound call the proxy makes.
- **Streaming body capture** — both downstream and upstream logs now defer
  recording until the stream ends, so `response_body` is populated for
  streaming requests. Controlled by `enable_downstream_log_body` /
  `enable_upstream_log_body` config.
- **Auto-check for updates** — the console fires a background version check
  after admin login and shows a toast when a new release is available.
- **Wildcard model permission for admins** — creating or promoting a user to
  admin now automatically seeds a `*` model permission so the admin can call
  all providers immediately.
- **Credential import via raw JSON** — the console credential form now offers
  a single JSON textarea for direct paste import; plain cookie or API-key
  strings are auto-wrapped into the correct JSON shape.

#### Fixed

- **Credential token refresh persisted** — refreshed `access_token` values
  (from `refresh_token` on 401/403) are now written back to the database and
  updated in memory, so they survive restarts.
- **Cookie-only credentials** — credentials with only a `cookie` field (no
  `access_token`) can now be deserialized; bootstrap populates the token.
- **Claude Code org info backfill** — `billing_type`, `rate_limit_tier`,
  `account_uuid`, and `user_email` are now extracted from the bootstrap
  /organizations response when the token endpoint omits them.
- **Version check endpoint** — the updater now uses the GitHub Releases API
  instead of a nonexistent `latest.json` manifest URL.
- **Console session stability** — 401 responses from upstream provider routes
  no longer incorrectly clear the admin session; only `/admin/*` and `/login`
  401s trigger logout.
- **Request log loading loop** — removed `pageCursors` from the row-loading
  effect dependency array to break an infinite re-render cascade.
- **Cache breakpoint TTL aliases** — `"5m"` and `"1h"` are now accepted as
  serde aliases alongside `"ttl5m"` / `"ttl1h"`.
- **Credential quota reset time** — displayed in local timezone via
  `toLocaleString()` instead of raw ISO strings.
- **Credential card layout** — title, badge, and action buttons now wrap
  cleanly; expanded details flow below on their own line.
- **Android CI** — updated `setup-android` action to v4.

#### Changed

- **`subscription_type` removed** — `subscription_type` / `billing_type` /
  `organization_type` fields are dropped from credential, cookie exchange,
  OAuth profile, and frontend forms. Only `rate_limit_tier` is retained.
- **Cache breakpoint simplification** — `content_position` / `content_index`
  removed from breakpoint rules; breakpoints now always use flat block
  positioning across all messages.
- **i18n** — shortened Chinese cache breakpoint position labels
  (正数 / 倒数).

### 中文

#### 新增

- **上游请求日志** — 配额查询和 cookie 交换的每一步 HTTP 请求现在都会记录到
  `upstream_requests` 表，完整追踪代理发出的所有出站调用。
- **流式响应 body 采集** — 下游和上游日志均推迟到流结束后再写入，流式请求的
  `response_body` 不再为空。由 `enable_downstream_log_body` /
  `enable_upstream_log_body` 配置控制。
- **自动检查更新** — 管理员登录后控制台会在后台检查新版本，有新版时弹出提示。
- **管理员自动授权通配符模型权限** — 新建或提升为 admin 的用户会自动获得 `*`
  模型权限，无需手动配置即可调用所有 provider。
- **凭证 JSON 粘贴导入** — 控制台凭证表单新增单个 JSON 文本框，支持直接粘贴
  完整 JSON；也可粘贴纯 cookie 或 API key 字符串，自动包装为正确格式。

#### 修复

- **凭证 token 刷新落库** — 通过 refresh_token 刷新的 access_token 现在会
  同时更新内存和写入数据库，重启后不丢失。
- **纯 cookie 凭证** — 仅含 `cookie` 字段（无 `access_token`）的凭证现在可以
  正常反序列化，bootstrap 流程会自动补全 token。
- **Claude Code 组织信息回填** — 当 token 端点未返回组织信息时，
  `billing_type`、`rate_limit_tier`、`account_uuid`、`user_email` 会从
  bootstrap /organizations 响应中提取并回填。
- **版本检查端点** — 更新检查改用 GitHub Releases API，不再请求不存在的
  `latest.json`。
- **控制台会话稳定性** — 上游 provider 路由返回的 401 不再误触发管理员登出，
  仅 `/admin/*` 和 `/login` 路径的 401 才清除会话。
- **请求日志加载死循环** — 从行加载 effect 的依赖数组中移除 `pageCursors`，
  打破无限重渲染循环。
- **缓存断点 TTL 别名** — `"5m"` 和 `"1h"` 现在可以作为 serde 别名使用，
  与 `"ttl5m"` / `"ttl1h"` 等效。
- **凭证配额重置时间** — 使用 `toLocaleString()` 显示本地时区，不再显示原始
  ISO 字符串。
- **凭证卡片布局** — 标题、标记和操作按钮正确换行；展开详情独占整行显示。
- **Android CI** — `setup-android` action 升级到 v4。

#### 变更

- **移除 `subscription_type`** — 从凭证、cookie 交换、OAuth profile 和前端
  表单中删除 `subscription_type` / `billing_type` / `organization_type`
  字段，仅保留 `rate_limit_tier`。
- **缓存断点简化** — 移除 breakpoint 规则中的 `content_position` /
  `content_index`，断点统一使用跨所有消息的扁平 block 定位。
- **国际化** — 缩短中文缓存断点位置标签（正数 / 倒数）。

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
