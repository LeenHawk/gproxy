---
title: 快速开始
description: 5 分钟内用一个供应商、一个用户和一个 API 密钥把 GPROXY 跑起来。
---

本页帮你从零跑到一个真正可用的 GPROXY：连接上游、创建管理员账号，以及一个可以发请求的用户 API key。

## 1. 编写种子 TOML 配置

`GPROXY_CONFIG` 指向的 TOML 文件只在**首次**启动时使用 —— 它负责初始化数据库。
之后数据库就是唯一的事实来源，所有修改都走控制台或管理 API。

把下面内容保存为 `gproxy.toml`。它会建立一个上游供应商、一个真实模型，
以及一个带**通配权限**的 **admin 用户** —— 足以立即登录控制台并发起请求。

```toml
[global]
host = "127.0.0.1"
port = 8787
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"

[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [
  { api_key = "sk-your-upstream-key" }
]

[[models]]
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
display_name = "GPT-4.1 mini"
enabled = true
price_each_call = 0.0

# 管理员账号 —— 用于登录控制台和访问 /admin/* API。
[[users]]
name = "admin"
password = "change-me"
is_admin = true
enabled = true

[[users.keys]]
api_key = "sk-admin-1"
label = "default"
enabled = true

# 通配权限：admin 可以调用任意供应商上的任意模型。
[[permissions]]
user_name = "admin"
model_pattern = "*"
```

:::tip
只要种子里已经存在 `is_admin = true` 且至少有一张启用 key 的用户 (例如
上面这个)，就会**完全跳过** `GPROXY_ADMIN_*` bootstrap。非 admin 的普通
用户之后可以在控制台里创建。
:::

该文件支持的全部字段见 [TOML 配置参考](/zh-cn/reference/toml-config/)。

## 2. 启动 GPROXY

```bash
GPROXY_CONFIG=./gproxy.toml ./target/release/gproxy
```

首次启动时 GPROXY 会：

1. 自动创建 `./data/gproxy.db` (SQLite)。
2. 把种子 TOML 导入数据库。
3. 在 `127.0.0.1:8787` 启动 HTTP 服务。

因为种子里已经定义了管理员账号，**不需要**再设置 `GPROXY_ADMIN_USER` /
`GPROXY_ADMIN_PASSWORD` / `GPROXY_ADMIN_API_KEY`。这三个环境变量只在种子
没有管理员时才会被用到。

:::tip
如果你不使用种子 TOML，可以改为设置上述三个环境变量，让 GPROXY 在首次启动时
bootstrap 一个管理员。未设置时 GPROXY 会自动生成密码和 API key 并
**打印一次** —— 请立刻记下。
:::

## 3. 打开控制台

浏览器访问 **<http://127.0.0.1:8787/console>**，用 `admin` / `change-me`
登录，你应当看到：

- 种子里的 `openai-main` 供应商。
- `gpt-4.1-mini` 出现在它的模型列表中。
- 用户 `admin`，附带 key `sk-admin-1` 与一条通配权限。

之后可以在控制台的*用户*标签页中创建非 admin 的普通用户并限定其模型访问 ——
详见 [用户与 API 密钥](/zh-cn/guides/users-and-keys/) 和
[权限、限流与配额](/zh-cn/guides/permissions/)。

## 4. 发送第一个请求

现在可以发一个真正的请求了。管理员的 API key 和普通用户的 key 一样能用在
LLM 路由上。

完整示例 (包括如何使用模型别名、以及 Claude / Gemini 兼容接口的用法) 请见
[发送第一个请求](/zh-cn/getting-started/first-request/)。
