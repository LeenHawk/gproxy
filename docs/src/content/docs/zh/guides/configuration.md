---
title: 配置说明
description: 多数据库、原生渠道、自定义渠道与 dispatch 转换配置。
---

## 配置入口

推荐从以下文件开始：

- `gproxy.example.toml`：最小可运行示例
- `gproxy.example.full.toml`：全量字段示例

## 配置优先级

运行时优先级：

`CLI 参数 / 环境变量 > gproxy.toml > 默认值`

说明：

- 数据库已初始化后，默认优先数据库状态（除非通过下方启动开关强制按配置文件覆盖）。

常用覆盖项：

- `--config` / `GPROXY_CONFIG_PATH`
- `--host` / `GPROXY_HOST`
- `--port` / `GPROXY_PORT`
- `--proxy` / `GPROXY_PROXY`
- `--admin-key` / `GPROXY_ADMIN_KEY`
- `--bootstrap-force-config` / `GPROXY_BOOTSTRAP_FORCE_CONFIG`
- `--mask-sensitive-info` / `GPROXY_MASK_SENSITIVE_INFO`
- `--data-dir` / `GPROXY_DATA_DIR`
- `--dsn` / `GPROXY_DSN`
- `--database-secret-key` / `DATABASE_SECRET_KEY`

## 启动数据来源模式

启动期开关（仅 CLI/ENV，非 `gproxy.toml` 字段）：

- `--bootstrap-force-config` / `GPROXY_BOOTSTRAP_FORCE_CONFIG`

行为：

- 默认（`false` 或未设置）：
  - 若数据库未初始化，按 `gproxy.toml` 引导；
  - 若数据库已初始化，优先数据库状态，并跳过配置文件中的渠道/provider 导入；
  - 启动时提供的 `admin_key` 覆盖仍生效。
- `true`：
  - 启动时强制应用配置文件中的 channels/settings/credentials/global；
  - 适用于明确要用配置文件覆盖现有数据库引导状态的场景。

## 多数据库支持（重点）

`gproxy-storage` 已启用 `sqlite + mysql + postgres` 驱动。你只要改 `global.dsn` 即可切换。

示例：

```toml
# SQLite（默认）
dsn = "sqlite://./data/gproxy.db?mode=rwc"
```

```toml
# MySQL
dsn = "mysql://user:password@127.0.0.1:3306/gproxy"
```

```toml
# PostgreSQL
dsn = "postgres://user:password@127.0.0.1:5432/gproxy"
```

## 数据库静态加密

可通过 CLI 或环境变量设置数据库静态加密密钥：

```bash
./gproxy --database-secret-key 'replace-with-long-random-string'
```

```bash
export DATABASE_SECRET_KEY='replace-with-long-random-string'
./gproxy
```

行为说明：

- 未设置 `DATABASE_SECRET_KEY`：敏感字段按明文读写数据库；
- 已设置 `DATABASE_SECRET_KEY`：会对 `credential.secret_json`、用户 API Key、用户密码、`admin_key` 与 `hf_token` 做透明静态加密。

使用建议：

- 尽量在首次初始化数据库前就设置该密钥，并在连接同一数据库的所有实例上保持一致；
- 使用免费额度或共享型托管数据库时，强烈建议设置该密钥，避免敏感字段明文落库；
- 建议通过平台 Secret / 环境变量注入，不要把密钥写进仓库或公开配置；
- 如果数据库里已经写入密文，不要直接更换该值；如需轮换，请先做数据迁移或重加密。

## `global`

| 字段 | 说明 |
|---|---|
| `host` | 监听地址，默认 `127.0.0.1` |
| `port` | 监听端口，默认 `8787` |
| `proxy` | 上游代理；空字符串表示禁用 |
| `hf_token` | 可选，HuggingFace token |
| `hf_url` | HuggingFace 基址，默认 `https://huggingface.co` |
| `admin_key` | 管理员 key；为空时首次可自动生成 |
| `mask_sensitive_info` | 是否在日志/事件中脱敏敏感字段 |
| `data_dir` | 数据目录，默认 `./data` |
| `dsn` | 数据库 DSN（sqlite/mysql/postgres） |

## `runtime`

| 字段 | 默认值 | 说明 |
|---|---:|---|
| `storage_write_queue_capacity` | `4096` | 存储写入队列容量 |
| `storage_write_max_batch_size` | `1024` | 单批次最大写入事件数 |
| `storage_write_aggregate_window_ms` | `25` | 聚合窗口（毫秒） |

## `channels`（原生与自定义）

每个通道使用 `[[channels]]` 声明，常见字段：

- `id`：通道 ID（内置如 `openai`，或自定义如 `mycustom`）
- `enabled`：是否启用
- `settings`：通道配置（通常至少包含 `base_url`）
- `dispatch`：可选协议分发规则
- `credentials`：凭证列表（支持多凭证）

示例：

```toml
[[channels]]
id = "openai"
enabled = true

[channels.settings]
base_url = "https://api.openai.com"

[[channels.credentials]]
id = "openai-main"
label = "primary"
secret = "sk-replace-me"
```

## 内置渠道能力矩阵（重点）

| 渠道 | `id` | OAuth | `/v1/usage` | `secret` 凭证 |
|---|---|---|---|---|
| OpenAI | `openai` | 否 | 否 | 是 |
| Anthropic | `anthropic` | 否 | 否 | 是 |
| AiStudio | `aistudio` | 否 | 否 | 是 |
| VertexExpress | `vertexexpress` | 否 | 否 | 是 |
| Vertex | `vertex` | 否 | 否 | 否（service account） |
| GeminiCli | `geminicli` | 是 | 是 | 否（OAuth builtin） |
| ClaudeCode | `claudecode` | 是 | 是 | 否（OAuth/Cookie builtin） |
| Codex | `codex` | 是 | 是 | 否（OAuth builtin） |
| Antigravity | `antigravity` | 是 | 是 | 否（OAuth builtin） |
| Nvidia | `nvidia` | 否 | 否 | 是 |
| Deepseek | `deepseek` | 否 | 否 | 是 |
| Groq | `groq` | 否 | 否 | 是 |

## Claude / ClaudeCode 缓存改写（`cache_breakpoints`）

`anthropic` 与 `claudecode` 通过 `channels.settings.cache_breakpoints` 控制 cache-control 改写。

规则模型：

- 配置键：`channels.settings.cache_breakpoints`
- 值类型：数组，最多 `4` 条规则
- `target` 支持：
  - `top_level`（别名：`global`）
  - `tools`
  - `system`
  - `messages`
- 对非 `top_level` 目标：
  - `position`：`nth` 或 `last_nth`
  - `index`：从 1 开始
  - 对 `messages`，索引基于扁平化后的 `messages[*].content` block；`content: "..."` 会先规范化为一个 text block
  - 对 `messages`，如果配置了 `content_position` / `content_index`，则 `position` / `index` 先选 message，再由 `content_*` 在该 message 内选 block
- `top_level` 目标会忽略 `position` / `index`
- `ttl`：`auto` | `5m` | `1h`
  - `auto` 会注入 `{"type":"ephemeral"}`（不带 `ttl`）

改写行为：

- 请求里已有的 `cache_control` 会被保留，并计入 4 条上限
- gproxy 只会填充剩余槽位，不会覆盖已有 top-level/block `cache_control`
- magic trigger 触发的注入也共用这 4 条预算
- 仅对 `anthropic` / `claudecode` 的消息生成请求生效
- 管理端会先按 `top_level -> tools -> system -> messages` 排序，再由服务端截断前 4 条

`ttl` 省略（`auto`）时的默认 TTL 说明：

- `anthropic`：上游默认按 `5m`
- `claudecode`：上游默认按 `5m`
- 若要行为可预测，建议显式写 `ttl = "5m"` 或 `ttl = "1h"`

示例：

```toml
[[channels]]
id = "anthropic"
enabled = true

[channels.settings]
base_url = "https://api.anthropic.com"
cache_breakpoints = [
  { target = "top_level", ttl = "auto" },
  { target = "system", position = "last_nth", index = 1, ttl = "auto" },
  { target = "messages", position = "last_nth", index = 11, ttl = "auto" },
  { target = "messages", position = "last_nth", index = 1, content_position = "last_nth", content_index = 1, ttl = "5m" }
]

[[channels]]
id = "claudecode"
enabled = true

[channels.settings]
base_url = "https://api.anthropic.com"
cache_breakpoints = [
  { target = "top_level", ttl = "auto" },
  { target = "messages", position = "last_nth", index = 1, content_position = "last_nth", content_index = 1, ttl = "1h" }
]
```

## 自定义渠道配置示例（重点）

```toml
[[channels]]
id = "mycustom"
enabled = true

[channels.settings]
base_url = "https://api.example.com"

[[channels.credentials]]
secret = "custom-provider-api-key"
```

说明：

- 自定义渠道默认走 `ProviderDispatchTable::default_for_custom()`
- 你也可以在配置里显式提供 `dispatch`，做精细化协议路由

## `channels.credentials`

可用字段：

- `id` / `label`：可读标识
- `secret`：API Key 通道
- `builtin`：OAuth / ServiceAccount 结构化凭证
- `state`：健康状态种子

健康状态类型：

- `healthy`：可用
- `partial`：模型级冷却（部分可用）
- `dead`：不可用

## 凭证选择模式

在 `channels.settings` 里，可通过以下两个字段控制多凭证路由：

- `credential_round_robin_enabled`
- `credential_cache_affinity_enabled`
- `credential_cache_affinity_max_keys`

完整行为矩阵、内部亲和池设计、命中判定机制，以及 OpenAI/Claude/Gemini 上游缓存命中建议，见：

- [凭证选择与缓存亲和池](/zh/guides/credential-selection-cache-affinity/)

## dispatch 与转换能力

`dispatch` 决定“请求进入后如何被实现”：

- `Passthrough`：原样转发给上游
- `TransformTo`：转换为目标协议再转发
- `Local`：本地实现（例如某些计数能力）
- `Unsupported`：显式不支持

这也是 GPROXY 同时支持多协议入口、多上游原生差异的核心机制。
