---
title: 配置参考
description: CLI 参数、环境变量、TOML 配置、数据库加密与多数据库支持的完整参考。
---

## 配置优先级

GPROXY 按以下顺序从三个来源解析配置：

1. **CLI 参数 / 环境变量** -- 始终优先。
2. **TOML 配置文件**（`gproxy.toml`）-- 仅在首次引导时应用。
3. **数据库** -- 通过 Admin API 或控制台的所有运行时变更都持久化在这里。

数据库初始化后，后续启动会忽略 TOML 文件。所有运行时配置变更通过 Admin API 或控制台进行，并持久化到数据库。

启动参数 `--bootstrap-force-config` / `GPROXY_BOOTSTRAP_FORCE_CONFIG` 可以覆盖此行为：

- `false`（默认）：如果数据库已有数据，优先使用数据库状态，跳过 TOML 导入。
- `true`：启动时强制应用 TOML 中的 Channel、设置、凭据和全局值。用于有意从文件覆盖现有数据库状态的场景。

## CLI 参数和环境变量

| 参数 | 环境变量 | 默认值 | 说明 |
|----------|---------|---------|-------------|
| `--host` | `GPROXY_HOST` | `127.0.0.1` | 监听地址 |
| `--port` | `GPROXY_PORT` | `8787` | 监听端口 |
| `--admin-user` | `GPROXY_ADMIN_USER` | `admin` | 引导管理员用户名 |
| `--admin-password` | `GPROXY_ADMIN_PASSWORD` | （自动生成） | 引导管理员密码 |
| `--admin-api-key` | `GPROXY_ADMIN_API_KEY` | （自动生成） | 引导管理员 API key |
| `--dsn` | `GPROXY_DSN` | （无；默认 SQLite） | 数据库连接字符串 |
| `--config` | `GPROXY_CONFIG` | `gproxy.toml` | TOML 配置文件路径 |
| `--data-dir` | `GPROXY_DATA_DIR` | `./data` | 数据目录 |
| `--proxy` | `GPROXY_PROXY` | （无） | 上游 HTTP 代理 |
| `--spoof-emulation` | `GPROXY_SPOOF` | `chrome_136` | TLS 指纹模拟 |
| `--database-secret-key` | `DATABASE_SECRET_KEY` | （无） | XChaCha20Poly1305 加密密钥，用于数据库静态加密 |

未指定 `--admin-password` 或 `--admin-api-key` 时，GPROXY 自动生成随机值并在启动时输出日志。务必保存——不会再次显示。

如果管理员用户名、密码或 API key 通过 CLI/环境变量显式提供（即使数据库已有数据），引导管理员会在每次启动时进行协调。

## TOML 配置参考

TOML 配置文件在首次启动时将数据写入数据库。字段直接映射到数据库表。初始写入后，通过 Admin API 或控制台管理一切。

### `[global]`

全局服务器设置。

| 字段 | 默认值 | 说明 |
|-------|---------|-------------|
| `host` | `127.0.0.1` | 监听地址 |
| `port` | `8787` | 监听端口 |
| `proxy` | （无） | 上游 HTTP 代理。空字符串表示禁用 |
| `spoof_emulation` | `chrome_136` | TLS 指纹模拟目标 |
| `update_source` | `github` | 自更新来源（`github` 或 `cloudflare`） |
| `enable_usage` | `true` | 是否记录 token 用量和费用 |
| `enable_upstream_log` | `false` | 是否记录上游请求/响应元数据 |
| `enable_upstream_log_body` | `false` | 是否记录上游请求/响应体 |
| `enable_downstream_log` | `false` | 是否记录下游（客户端）请求/响应元数据 |
| `enable_downstream_log_body` | `false` | 是否记录下游请求/响应体 |
| `dsn` | （从 data_dir 推导） | 数据库连接字符串 |
| `data_dir` | `./data` | SQLite 及其他文件的数据目录 |

```toml
[global]
host = "0.0.0.0"
port = 8787
proxy = ""
spoof_emulation = "chrome_136"
update_source = "github"
enable_usage = true
enable_upstream_log = false
enable_upstream_log_body = false
enable_downstream_log = false
enable_downstream_log_body = false
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"
```

### `[[providers]]`

每个 `[[providers]]` 块定义一个上游提供商实例。

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `name` | 是 | -- | 唯一提供商名称（如 `openai-prod`） |
| `channel` | 是 | -- | Channel 类型。内置：`openai`、`anthropic`、`aistudio`、`vertexexpress`、`vertex`、`geminicli`、`claudecode`、`codex`、`antigravity`、`nvidia`、`deepseek`、`groq`、`custom` |
| `settings` | 否 | `{}` | JSON 对象。大多数 Channel 至少需要 `base_url` |
| `credentials` | 否 | `[]` | 凭据 JSON 对象数组 |

```toml
[[providers]]
name = "openai-prod"
channel = "openai"

[providers.settings]
base_url = "https://api.openai.com"

[[providers.credentials]]
api_key = "sk-replace-me"
```

对于使用 OAuth 凭据的 Channel（如 `claudecode`、`geminicli`、`codex`、`antigravity`），凭据使用结构化的内置字段而非普通 API key。详见完整示例配置。

### `[[models]]`

覆盖或添加模型定价和显示元数据。

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `provider_name` | 是 | -- | 必须匹配某个提供商的 `name` |
| `model_id` | 是 | -- | 模型标识符（如 `gpt-4o`） |
| `display_name` | 否 | （无） | 人类可读的显示名称 |
| `enabled` | 否 | `true` | 是否启用该模型 |
| `price_each_call` | 否 | （无） | 每次请求的固定费用 |
| `price_tiers` | 否 | `[]` | 按输入 token 数分层定价 |

```toml
[[models]]
provider_name = "openai-prod"
model_id = "gpt-4o"
display_name = "GPT-4o"
enabled = true
price_each_call = 0.0
price_tiers = [
  { input_tokens_up_to = 128000, price_input_tokens = 2.5, price_output_tokens = 10.0 }
]
```

内置 Channel 自带默认模型定价表。显式的 `[[models]]` 条目会覆盖默认值。

### `[[model_aliases]]`

将别名模型名称映射到实际的提供商 + 模型。

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `alias` | 是 | -- | 客户端使用的别名 |
| `provider_name` | 是 | -- | 目标提供商名称 |
| `model_id` | 是 | -- | 目标模型 ID |
| `enabled` | 否 | `true` | 是否启用该别名 |

```toml
[[model_aliases]]
alias = "gpt-4"
provider_name = "openai-prod"
model_id = "gpt-4o"
enabled = true
```

### `[[users]]`

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `name` | 是 | -- | 用户名 |
| `password` | 否 | `""` | 明文密码或 Argon2 PHC 格式哈希 |
| `enabled` | 否 | `true` | 是否允许该用户认证 |
| `is_admin` | 否 | `false` | 管理员角色标记 |

每个用户可以包含嵌套的 API key：

#### `[[users.keys]]`

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `api_key` | 是 | -- | API key 字符串 |
| `label` | 否 | （无） | 人类可读的标签 |
| `enabled` | 否 | `true` | 是否启用该 Key |

```toml
[[users]]
name = "alice"
password = "plaintext-or-argon2-hash"
enabled = true
is_admin = false

[[users.keys]]
api_key = "sk-alice-key-1"
label = "dev"
enabled = true
```

密码可以是明文（导入时自动哈希）或 Argon2id PHC 格式的哈希字符串（原样存储）。

### `[[permissions]]`

通过模式匹配授予用户模型访问权限。

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `user_name` | 是 | -- | 必须匹配某个用户的 `name` |
| `provider_name` | 否 | （无） | 限定到特定提供商。`None` 表示所有提供商 |
| `model_pattern` | 是 | -- | Glob 模式。`*` 匹配所有模型 |

```toml
[[permissions]]
user_name = "alice"
model_pattern = "*"

[[permissions]]
user_name = "bob"
provider_name = "openai-prod"
model_pattern = "gpt-*"
```

没有任何权限的用户无法调用任何模型。

### `[[file_permissions]]`

授予用户针对特定提供商的文件上传权限。

| 字段 | 必填 | 说明 |
|-------|----------|-------------|
| `user_name` | 是 | 必须匹配某个用户的 `name` |
| `provider_name` | 是 | 必须匹配某个提供商的 `name` |

```toml
[[file_permissions]]
user_name = "alice"
provider_name = "anthropic-prod"
```

### `[[rate_limits]]`

按用户和模型模式的速率限制。

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `user_name` | 是 | -- | 必须匹配某个用户的 `name` |
| `model_pattern` | 是 | -- | 受影响模型的 Glob 模式 |
| `rpm` | 否 | （无） | 每分钟请求数 |
| `rpd` | 否 | （无） | 每天请求数 |
| `total_tokens` | 否 | （无） | 总 token 预算（终身） |

```toml
[[rate_limits]]
user_name = "alice"
model_pattern = "*"
rpm = 60
rpd = 1000
```

省略某个字段表示该维度无限制。

### `[[quotas]]`

按用户的费用配额。

| 字段 | 必填 | 默认值 | 说明 |
|-------|----------|---------|-------------|
| `user_name` | 是 | -- | 必须匹配某个用户的 `name` |
| `quota` | 是 | -- | 最大允许费用（USD） |
| `cost_used` | 否 | `0.0` | 当前已消耗费用 |

```toml
[[quotas]]
user_name = "alice"
quota = 50.0
cost_used = 0.0
```

当 `cost_used >= quota` 时，请求会被拒绝。

## 数据库加密

设置 `--database-secret-key` 或 `DATABASE_SECRET_KEY` 以启用数据库敏感字段的静态加密。

### 加密范围

- 凭据密钥（API key、OAuth token、服务账号密钥）
- 用户 API key
- 用户密码哈希
- 管理员 API key

### 算法

使用 XChaCha20Poly1305 配合 Argon2id 派生的 256 位密钥。你提供的密钥不会直接使用——而是通过 Argon2id（19 MiB 内存、2 次迭代、1 通道）加固定域分隔符 salt 派生实际加密密钥。

加密字符串以 `enc:v2:` 前缀存储。加密 JSON 值存储为包含 `$gproxy_enc`、`nonce` 和 `ciphertext` 字段的对象。未加密的值在读取时透明传递。

### 使用规则

- 在**首次引导前**设置密钥。写入数据库的所有敏感值都会被加密。
- 共享同一数据库的所有实例**必须使用相同的密钥**。
- 通过环境变量或平台密钥管理注入。不要将密钥提交到源代码或配置文件中。
- 数据写入后不要更改密钥。更改密钥需要迁移/重新加密方案。
- 未设置密钥时，敏感字段以明文存储。

```bash
# 通过环境变量设置
export DATABASE_SECRET_KEY='your-long-random-secret-string'
./gproxy

# 或通过 CLI 参数
./gproxy --database-secret-key 'your-long-random-secret-string'
```

## 多数据库支持

GPROXY 支持 SQLite、MySQL 和 PostgreSQL。数据库后端由 DSN 格式决定。

### SQLite（默认）

未指定 DSN 时，GPROXY 在 `{data_dir}/gproxy.db` 创建 SQLite 数据库。

```
sqlite://./data/gproxy.db?mode=rwc
```

### MySQL

```
mysql://user:password@127.0.0.1:3306/gproxy
```

### PostgreSQL

```
postgres://user:password@127.0.0.1:5432/gproxy
```

通过 `--dsn`、`GPROXY_DSN` 或 TOML 配置的 `[global] dsn` 设置 DSN。

数据库 Schema 在启动时自动同步，无需手动迁移。
