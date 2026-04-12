---
title: TOML 配置
description: 首次初始化时由 GPROXY_CONFIG 读取的种子 TOML 文件。
---

`GPROXY_CONFIG` (默认 `gproxy.toml`) 指向的文件是**种子配置**。它只在**首次**启动、
数据库中尚无数据时被读取。之后数据库就是事实来源 —— 修改 TOML 并重启不会重新导入。

确切的结构定义在
[`crates/gproxy-api/src/admin/config_toml.rs`](https://github.com/LeenHawk/gproxy/blob/main/crates/gproxy-api/src/admin/config_toml.rs)。
下文按表介绍每个字段。

## 示例

一个最小但真实可用的种子：一个供应商、一个真实模型、一个带通配权限的
**admin** 用户。其余 (更多供应商、普通用户、别名、限流、配额) 都可以事后
在控制台里补，也可以在首次启动前继续往这个文件里加。

```toml
[global]
host = "0.0.0.0"
port = 8787
proxy = "http://127.0.0.1:7890"
spoof_emulation = "chrome_136"
update_source = "github"
enable_usage = true
enable_upstream_log = false
enable_upstream_log_body = false
enable_downstream_log = false
enable_downstream_log_body = false
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"

[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [
  { api_key = "sk-provider-1" }
]

[[models]]
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
display_name = "GPT-4.1 mini"
enabled = true
price_each_call = 0.0

# 管理员 —— 用于登录控制台和调用 /admin/*
[[users]]
name = "admin"
password = "change-me"              # 明文或 Argon2 PHC
is_admin = true
enabled = true

[[users.keys]]
api_key = "sk-admin-1"
label = "default"
enabled = true

# 管理员的通配权限。
# 省略 `provider_name` 表示匹配所有供应商。
[[permissions]]
user_name = "admin"
model_pattern = "*"
```

### 添加一个非管理员用户

如果你想在种子里同时放一个普通用户，在同一个文件里追加类似这样的配置：

```toml
[[users]]
name = "alice"
password = "plain-text-or-argon2-phc"
enabled = true

[[users.keys]]
api_key = "sk-user-alice-1"
label = "default"
enabled = true

# 限定权限：alice 只能在 openai-main 上调用 gpt-*。
[[permissions]]
user_name = "alice"
provider_name = "openai-main"
model_pattern = "gpt-*"

[[rate_limits]]
user_name = "alice"
model_pattern = "gpt-*"
rpm = 60
rpd = 10000
total_tokens = 200000

[[quotas]]
user_name = "alice"
quota = 100.0
cost_used = 0.0

# 可选：别名、文件权限等。
[[model_aliases]]
alias = "chat-default"
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true

[[file_permissions]]
user_name = "alice"
provider_name = "openai-main"
```

## `[global]`

顶层运行态设置。每个字段都可省略；未指定时会回退到对应环境变量或内建默认值。

| 字段 | 说明 |
| --- | --- |
| `host`、`port` | 监听地址和端口。 |
| `proxy` | 调用上游时使用的 HTTP 代理。 |
| `spoof_emulation` | TLS 指纹伪装名。 |
| `update_source` | 自升级检查使用的来源 (如 `github`)。 |
| `enable_usage` | 是否开启用量记账。 |
| `enable_upstream_log` / `enable_upstream_log_body` | 是否捕获上游信封 / body。 |
| `enable_downstream_log` / `enable_downstream_log_body` | 是否捕获下游信封 / body。 |
| `dsn` | 数据库 DSN。 |
| `data_dir` | 数据目录。 |

## `[[providers]]`

每个条目代表一个上游供应商。

| 字段 | 说明 |
| --- | --- |
| `name` | 供应商唯一名。 |
| `channel` | 通道类型 (见 [供应商与通道](/zh-cn/guides/providers/))。 |
| `settings` | 传给通道 settings 类型的 JSON 值。 |
| `credentials` | JSON 值数组，每一项是该通道能识别的凭证。 |

`settings` 和 `credentials[i]` 都通过 `serde_json::Value` 反序列化 ——
具体 schema 取决于所选通道 (`OpenAiSettings`、`AnthropicCredential` 等)。

## `[[models]]` 与 `[[model_aliases]]`

`[[models]]` 定义一个可转发的真实模型并可带价格；`[[model_aliases]]` 定义一个
指向 `(provider, model_id)` 的别名。导入时两者都落入统一的 `models` 表
(别名在 `alias_of` 列上为非 NULL)。

## `[[users]]` 与 `[[users.keys]]`

`[[users]]` 定义账号。`password` 可以是明文 (导入时用 Argon2 哈希) 或直接是
Argon2 PHC 字符串。设 `is_admin = true` 即为管理员 —— 该账号可以登录控制台
并访问 `/admin/*`。

`[[users.keys]]` 是嵌套的 array-of-tables，列出该用户的 API key 列表，
每一项含 `api_key`、`label`、`enabled`。

:::tip
管理员用户调用 LLM 路由时仍然需要至少一条 `[[permissions]]` 行 ——
通常写一条通配行即可 (`model_pattern = "*"`，省略 `provider_name`)。
`is_admin` 控制的是管理接口的访问权限，而不是模型访问权限。
:::

## 访问控制表

| 表 | 作用 |
| --- | --- |
| `[[permissions]]` | `(user, provider, model_pattern)` 授予模型访问权限。 |
| `[[file_permissions]]` | 授予某供应商文件接口的访问权限。 |
| `[[rate_limits]]` | 按 `(user, pattern)` 的 `rpm` / `rpd` / `total_tokens` 上限。 |
| `[[quotas]]` | 按用户的 USD 上限 (`quota` / `cost_used`)。 |

语义和执行顺序见 [权限、限流与配额](/zh-cn/guides/permissions/)。

## Bootstrap 行为

启动时，gproxy 检查数据库是否已有数据：

- **空库：** 导入 TOML 种子。管理员账号要么来自种子 (`is_admin = true` 且有
  启用 key 的用户)，要么来自 `GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` /
  `GPROXY_ADMIN_API_KEY` 的 bootstrap。
- **非空库：** 完全忽略 TOML。直接从控制台或管理 API 修改线上状态。
