# App 层当前架构盘点

> 基于 `apps/gproxy`、`crates/gproxy-api`、`crates/gproxy-server`、`crates/gproxy-storage` 实际代码整理，截至 2026-04-05。

---

## 1. 启动与装配

### 1.1 入口

**文件：** `apps/gproxy/src/main.rs`

**启动流程：**

```
1. 初始化 tracing（日志）
2. 解析 CLI 参数（host, port, admin_key, dsn, config, data_dir, proxy, spoof_emulation, database_secret_key）
3. 解析 DSN → 默认 sqlite://./data/gproxy.db，检查 DB 中是否有持久化的 DSN 覆盖
4. 创建 SeaOrmStorage + 同步 schema
5. 加载/播种状态（三选一）：
   ├── DB 有数据 → reload_from_db()
   ├── 有 TOML 配置 → seed_from_toml() → 持久化到 DB → 加载到内存
   └── 都没有 → seed_defaults() 创建最小默认值
6. 处理 CLI 覆盖（admin_key 等写回 DB）
7. 无 admin_key 且无数据 → 生成 UUID v7 并输出到日志
8. 构建 axum Router + 启动 HTTP 监听（graceful shutdown）
```

### 1.2 Bootstrap（`crates/gproxy-api/src/bootstrap.rs`）

**`reload_from_db(state: &AppState)`** — 从 DB 全量原子重建内存状态：

| 步骤 | 数据 | 目标 |
|------|------|------|
| 1 | global_settings | `state.replace_config()` |
| 2 | providers + credentials | 构建 `GproxyEngine` → `state.replace_engine()` + provider_names / provider_channels / provider_credentials |
| 3 | users | `state.replace_users()` |
| 4 | user_keys | `state.replace_keys()`（解密后加载） |
| 5 | models | `state.replace_models()`（含 price_tiers 反序列化） |
| 6 | model_aliases | `state.replace_model_aliases()`（解析 provider name → id） |
| 7 | user_permissions | `state.replace_user_permissions()`（按 user_id 分组） |
| 8 | file_permissions | `state.replace_user_file_permissions()` |
| 9 | rate_limits | `state.replace_user_rate_limits()` |
| 10 | quotas | `state.replace_user_quotas()` |
| 11 | user_files | `state.replace_user_files()` |
| 12 | claude_files | `state.replace_claude_files()` |

**一致性保证：** 每个集合通过 `ArcSwap::store()` 原子替换，并发请求不会看到半更新状态。

**`seed_from_toml()`** — 解析 TOML 配置文件，先持久化到 DB 再加载到内存。使用合成 ID（provider_id = index+1, credential_id = provider_id*1000+index 等）。

### 1.3 Reload

`POST /admin/reload` → 调用 `reload_from_db(state)` → 原子替换所有 AppState 集合。

---

## 2. AppState 当前职责

**文件：** `crates/gproxy-server/src/app_state.rs`

### 2.1 字段清单

```rust
pub struct AppState {
    // 核心服务（创建后固定）
    engine: ArcSwap<GproxyEngine>,              // SDK 执行引擎（可热替换）
    storage: Arc<ArcSwap<SeaOrmStorage>>,        // DB 连接（可热替换，支持 DSN 切换）
    config: ArcSwap<GlobalConfig>,               // 全局配置

    // 只读集合（通过 ArcSwap 原子替换）
    users: ArcSwap<Vec<MemoryUser>>,                          // 用户注册表
    keys: ArcSwap<HashMap<String, MemoryUserKey>>,            // API key → 用户映射
    models: ArcSwap<Vec<MemoryModel>>,                        // 模型注册表（含定价）
    model_aliases: ArcSwap<HashMap<String, ModelAliasTarget>>, // 别名 → (provider_name, model_id)
    provider_names: ArcSwap<HashMap<String, i64>>,            // provider name → DB id
    provider_channels: ArcSwap<HashMap<String, String>>,      // provider name → channel type
    provider_credentials: ArcSwap<HashMap<String, Vec<i64>>>, // provider name → credential ID 列表

    // 文件相关
    user_files: ArcSwap<Vec<MemoryUserCredentialFile>>,           // 用户上传的文件
    claude_files: ArcSwap<HashMap<(i64, String), MemoryClaudeFile>>, // Claude 文件元数据缓存

    // 权限与限流
    user_permissions: ArcSwap<HashMap<i64, Vec<PermissionEntry>>>,      // 模型访问权限
    user_file_permissions: ArcSwap<HashMap<i64, Vec<FilePermissionEntry>>>, // 文件操作权限
    user_rate_limits: ArcSwap<HashMap<i64, Vec<RateLimitRule>>>,         // 限流规则

    // 可变运行时状态
    user_quotas: DashMap<i64, (f64, f64)>,  // 配额：(allocated, cost_used)
    pub rate_counters: RateLimitCounters,    // 请求计数器（RPM/RPD）
}
```

### 2.2 状态分类

| 字段 | 类型 | Runtime Primary | Cache Mirror |
|------|------|:-:|:-:|
| `engine` | `ArcSwap<GproxyEngine>` | ✓ | — |
| `storage` | `Arc<ArcSwap<SeaOrmStorage>>` | ✓ | — |
| `config` | `ArcSwap<GlobalConfig>` | ✓ | — |
| `users` | `ArcSwap<Vec<MemoryUser>>` | — | ✓（DB 镜像） |
| `keys` | `ArcSwap<HashMap<..>>` | — | ✓（DB 镜像，解密后） |
| `models` | `ArcSwap<Vec<MemoryModel>>` | — | ✓（DB 镜像） |
| `model_aliases` | `ArcSwap<HashMap<..>>` | — | ✓（DB 镜像） |
| `provider_names/channels/credentials` | `ArcSwap<HashMap<..>>` | — | ✓（DB 镜像的索引） |
| `user_files` / `claude_files` | `ArcSwap<..>` | — | ✓（DB 镜像） |
| `user_permissions` | `ArcSwap<HashMap<..>>` | — | ✓（DB 镜像） |
| `user_file_permissions` | `ArcSwap<HashMap<..>>` | — | ✓（DB 镜像） |
| `user_rate_limits` | `ArcSwap<HashMap<..>>` | — | ✓（DB 镜像） |
| `user_quotas` | `DashMap<i64, (f64, f64)>` | ✓（内存递增） | 部分（从 DB 加载初值） |
| `rate_counters` | `RateLimitCounters` | ✓（纯内存） | — |

**说明：**
- "Runtime Primary" = 运行时的权威来源
- "Cache Mirror" = 从 DB 加载的内存镜像，DB 是 source of truth
- `user_quotas` 混合模式：初值从 DB 加载，运行时通过 `DashMap` 原子递增 cost_used
- `rate_counters` 纯运行时，重启后归零

---

## 3. API 层职责

**文件：** `crates/gproxy-api/src/`

### 3.1 整体职责

`gproxy-api` 是 HTTP 入口层，负责：
- 路由组装（`router.rs`）
- 鉴权中间件桥接（`auth.rs`）
- 请求校验与 payload 解析
- 调用 SDK engine 执行代理
- 调用 storage 持久化写入
- 同步 AppState 内存状态

### 3.2 Admin Mutation 数据流

以 `upsert_provider` 为例：

```
POST /admin/providers/upsert
  ↓
1. require_admin_middleware → 校验 admin_key
  ↓
2. 校验 payload（channel 不可变、settings_json 符合 SDK 渠道 schema）
  ↓
3. DB-First 写入：
   state.storage().apply_write_event(
       StorageWriteEvent::UpsertProvider(payload)
   ).await
  ↓
4. 同步运行时：
   sync_provider_runtime(&state, &payload, previous_name)
   ├── 更新 AppState 内存索引（provider_names, provider_channels）
   ├── 从 DB 加载该 provider 的 credentials
   ├── 校验 credentials 并重建 GproxyEngine
   ├── 替换 provider_credentials 列表
   └── 恢复持久化的 credential health 状态
  ↓
5. 返回成功
```

**关键不变量：**
- **DB-First**：先写 DB 再更新内存，崩溃后可通过 `reload_from_db()` 恢复
- **Engine 重建**：Provider 变更需要完整重建 `GproxyEngine`（非增量）
- **内存一致性**：通过 `ArcSwap::store()` 或 `upsert_*()` 原子更新

### 3.3 Provider 请求代理数据流

```
POST /v1/chat/completions（或 /{provider}/v1/...）
  ↓
1. RequestBodyLimitLayer（50 MB）
  ↓
2. require_user_middleware → API key 鉴权 → 注入 AuthenticatedUser
  ↓
3. classify_middleware → 推断 (OperationFamily, ProtocolKind)
  ↓
4. request_model_middleware → 从 body 提取 model
  ↓
5. model_alias_middleware → 别名解析 → 替换为真实 (provider_name, model_id)
  ↓
6. sanitize_middleware → 请求清理
  ↓
7. handler::proxy / proxy_unscoped：
   ├── 校验模型权限（permission.rs）
   ├── 检查限流（rate_limit.rs）
   ├── 检查配额
   ├── 调用 engine.execute_request(ExecuteRequest { provider, operation, protocol, body })
   ├── 提取 usage / billing
   ├── 写入 usage + upstream_request + downstream_request（StorageWriteEvent）
   └── 返回响应（流式或非流式）
```

### 3.4 User / Admin / Provider 路由分层

| 层 | 鉴权 | 职责 | Middleware 栈 |
|---|-------|------|-------------|
| Admin | admin_key | CRUD 管理所有资源 | `require_admin_middleware` |
| User | user API key | 查询自身 key/quota/usage | `require_user_middleware` |
| Provider HTTP | user API key | LLM API 代理 | user auth + classify + model + alias + sanitize |
| Provider WS | user API key | WebSocket 代理 | user auth + sanitize |
| Provider Admin | admin_key | OAuth + 上游用量 | `require_admin_middleware` |

---

## 4. 存储层职责

**文件：** `crates/gproxy-storage/src/`

### 4.1 整体职责

`gproxy-storage` 封装了 SeaORM 数据库操作，提供：
- **写入模型**：`StorageWriteEvent` 枚举 + `StorageWriteBatch` 批处理
- **查询接口**：按资源类型组织的 query 模块
- **加密**：可选 XChaCha20Poly1305 字段级加密
- **Schema 同步**：启动时自动建表

### 4.2 写入模型

**模式：同步直写（DB-First）。**

```rust
pub enum StorageWriteEvent {
    UpsertGlobalSettings(GlobalSettingsWrite),
    UpsertProvider(ProviderWrite),
    DeleteProvider { id: i64 },
    UpsertCredential(CredentialWrite),
    DeleteCredential { id: i64 },
    // ... 共 26 个变体
    UpsertUsage(UsageWrite),
}
```

**`StorageWriteBatch`** — 将多个 event 按表分组，在单个事务中执行（先 delete 后 upsert，256 条一个 chunk）。

**调用方式：**
- 单条：`storage.apply_write_event(event).await`
- 批量：`storage.apply_write_batch(batch).await`

当前**没有异步写入队列**（`cf1a608 Remove obsolete storage write queue code` 已移除）。所有 DB 写入都是同步阻塞的 `.await`。

### 4.3 数据库表

| 表 | 持久化内容 | 说明 |
|---|-----------|------|
| `global_settings` | 服务器配置 | 单例（id=1） |
| `providers` | Provider 注册 | name, channel, settings_json, dispatch_json |
| `credentials` | Provider 凭证 | provider_id, kind, secret_json（加密）, enabled |
| `credential_statuses` | 凭证健康状态 | health_kind, health_json, last_error, checked_at |
| `users` | 用户 | name, password（加密）, enabled |
| `user_keys` | API Key | api_key（加密）, label, enabled |
| `models` | 模型注册 | provider_id, model_id, display_name, pricing |
| `model_aliases` | 模型别名 | alias → (provider_id, model_id) |
| `user_model_permissions` | 模型权限 | user_id, provider_id?, model_pattern |
| `user_file_permissions` | 文件权限 | user_id, provider_id |
| `user_rate_limits` | 限流规则 | user_id, model_pattern, rpm, rpd, total_tokens |
| `user_quotas` | 配额 | user_id, quota, cost_used |
| `user_credential_files` | 用户上传文件 | (user_id, provider_id, file_id), credential_id |
| `claude_files` | Claude 文件元数据 | provider_id, file_id, metadata JSON |
| `usages` | 用量日志 | trace_id, tokens, model, provider, user |
| `upstream_requests` | 上游请求日志 | request/response 完整内容 |
| `downstream_requests` | 下游请求日志 | request/response 完整内容 |

### 4.4 查询模块

```
query/
├── common.rs              Scope<T> 枚举（Eq/In/Any）
├── users.rs               用户查询
├── credentials.rs         凭证查询 + CredentialQueryRow
├── providers.rs           Provider 查询
├── models.rs              模型查询
├── model_aliases.rs       别名查询
├── user_permissions.rs    权限查询
├── user_file_permissions.rs  文件权限查询
├── user_rate_limits.rs    限流规则查询
├── global_settings.rs     全局设置查询
├── files.rs               文件查询
├── requests.rs            请求日志查询
└── usages.rs              用量日志查询 + 聚合
```

特殊查询：`list_user_keys_for_memory()` — 返回解密后的 API key 供 AppState 内存加载。

### 4.5 加密

可选启用（`DATABASE_SECRET_KEY` 环境变量）：
- 算法：XChaCha20Poly1305
- 加密字段：`admin_key`、`user.password`、`user_keys.api_key`、`credentials.secret_json`

---

## 5. 当前状态模型

### 5.1 数据所在层

| 数据 | 内存（AppState） | SDK（ProviderStore） | DB | 权威来源 |
|------|:-:|:-:|:-:|------|
| provider 定义 | provider_names/channels 索引 | ProviderInstance | providers 表 | **DB** |
| credential 列表 | provider_credentials 索引 | credentials Vec | credentials 表 | **DB** |
| credential 健康状态 | — | health Mutex | credential_statuses 表 | **SDK 内存**（DB 定期快照） |
| user | users Vec | — | users 表 | **DB** |
| user key | keys HashMap | — | user_keys 表 | **DB** |
| model 定义 | models Vec | — | models 表 | **DB** |
| model alias | model_aliases HashMap | — | model_aliases 表 | **DB** |
| user permission | user_permissions HashMap | — | user_model_permissions 表 | **DB** |
| file permission | user_file_permissions HashMap | — | user_file_permissions 表 | **DB** |
| rate limit 规则 | user_rate_limits HashMap | — | user_rate_limits 表 | **DB** |
| rate limit 计数 | rate_counters | — | — | **纯内存** |
| user quota 配置 | user_quotas DashMap | — | user_quotas 表 | **DB**（初值） |
| user quota cost_used | user_quotas DashMap | — | user_quotas 表 | **内存**（运行时递增，定期持久化） |
| request log | — | — | downstream/upstream_requests 表 | **DB** |
| usage log | — | — | usages 表 | **DB** |
| config | config GlobalConfig | — | global_settings 表 | **DB** |
| affinity 绑定 | — | CacheAffinityPool | — | **纯内存** |
| user files | user_files Vec | — | user_credential_files 表 | **DB** |
| claude files | claude_files HashMap | — | claude_files 表 | **DB** |

### 5.2 Mutation 模型：DB-First

当前所有 admin mutation 遵循 **DB-First** 模式：

```
API 请求 → 校验 → 写 DB → 同步内存 → 返回
```

**优点：** 崩溃安全，`reload_from_db()` 可完整恢复。  
**代价：** 每次 mutation 都是同步 DB 写 + 内存同步，provider 变更还需重建整个 `GproxyEngine`。

### 5.3 读取模型：Memory-First

所有请求处理的读取路径走内存：
- 鉴权：`AppState.authenticate_api_key()` → `keys` HashMap
- 权限校验：`user_permissions` HashMap → regex 匹配
- 限流检查：`user_rate_limits` + `rate_counters`
- 模型别名：`model_aliases` HashMap
- 配额检查：`user_quotas` DashMap

DB 在读路径上不参与。

### 5.4 DB 与内存的角色

| | DB | 内存 |
|---|---|---|
| **写** | 所有 mutation 的第一步 | mutation 第二步（同步） |
| **读** | admin 查询（query 路由） | 请求处理热路径 |
| **恢复** | `reload_from_db()` 的数据源 | — |
| **日志** | usage/request 日志的唯一落点 | 不缓存日志 |

### 5.5 架构债现状描述

以下是当前代码中可观察到的架构紧张点（仅描述现状，不展开改造建议）：

1. **Provider 变更需重建整个 GproxyEngine**
   更新单个 provider 的设置或凭证时，当前实现需要重建包含所有 provider 的 engine。无增量更新能力。

2. **双写耦合**
   Provider/credential CRUD 需要同时写 DB 和同步 SDK ProviderStore。两步之间没有事务保证——如果 DB 写成功但 SDK 同步失败（如 schema 校验不过），状态会不一致，需要手动 reload。

3. **配额 cost_used 持久化时机不确定**
   `user_quotas.cost_used` 在内存中原子递增，但持久化到 DB 的时机依赖 admin 操作（`UpsertUserQuota`）。如果进程崩溃，两次持久化之间的 cost_used 增量会丢失。

4. **限流计数器纯内存**
   `rate_counters` 重启归零。RPD（每日请求数）在进程重启后计数器清零。

5. **Credential 健康状态双向同步**
   SDK 的 `ProviderStore` 维护运行时健康状态，App 层通过 `credential_statuses` 表持久化。bootstrap 时从 DB 恢复到 SDK。但运行时 SDK 内部的健康变化（如 429 cooldown 过期）不会自动写回 DB——仅在 admin 显式操作时持久化。

6. **内存集合无部分更新**
   大部分 AppState 集合使用 `ArcSwap<Vec<T>>` 或 `ArcSwap<HashMap<K, V>>`，单条 CRUD 需要 clone 整个集合、修改、原子替换。数据量大时有性能开销。

7. **异步写入队列已移除**
   原有的 `StorageWriteQueue`（异步批量写 DB）已在 `cf1a608` 中移除。当前所有 DB 写入都是同步的 `.await`，包括高频的 usage/request 日志。
