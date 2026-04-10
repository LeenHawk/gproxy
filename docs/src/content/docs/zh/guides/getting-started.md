---
title: 快速开始
description: 从首次运行到发出第一个请求，不超过 5 分钟。
---

## 1. 下载二进制文件

从 [GitHub Releases](https://github.com/LeenHawk/gproxy/releases) 下载适合你平台的最新版本。

## 2. 启动

```bash
./gproxy
```

首次运行且没有配置文件和现有数据库时，GPROXY 会：

1. 创建 `./data/gproxy.db`（SQLite）。
2. 生成随机的管理员密码和 API key。
3. 将两者输出到标准输出。

注意日志中的以下内容：

```
INFO generated bootstrap admin password (save this!) admin_user="admin" admin_password="..."
INFO generated bootstrap admin API key (save this!) admin_user="admin" admin_api_key="..."
```

务必保存这些信息。密码用于控制台登录，API key 用于 API 访问。

## 3. 通过 CLI 参数覆盖默认值

你可以覆盖任何引导参数：

```bash
./gproxy \
  --admin-user myuser \
  --admin-password mysecretpassword \
  --admin-api-key sk-my-admin-key \
  --host 0.0.0.0 \
  --port 9090 \
  --dsn "postgres://user:pass@localhost/gproxy"
```

所有参数都支持环境变量：`GPROXY_HOST`、`GPROXY_PORT`、`GPROXY_ADMIN_USER`、`GPROXY_ADMIN_PASSWORD`、`GPROXY_ADMIN_API_KEY`、`GPROXY_DSN`、`GPROXY_DATA_DIR`、`GPROXY_PROXY`、`GPROXY_CONFIG`、`GPROXY_SPOOF`。

## 4. 验证 API

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer <admin-api-key>"
```

如果返回 JSON 模型列表，说明认证和路由已正常工作。

## 5. 打开控制台

在浏览器中访问 `http://127.0.0.1:8787/`，会自动重定向到 `/console/login`。使用第 2 步中的管理员用户名和密码登录。

在控制台中可以添加提供商、管理用户和 API key、查看用量、配置模型路由。

## 6. 添加第一个提供商

### 方式 A：控制台 UI

在控制台中进入「提供商」页面，添加新的提供商。选择 Channel（如 `openai`），设置 base URL，并添加至少一个包含 API key 的凭据。

### 方式 B：TOML 种子文件

在工作目录下创建 `gproxy.toml`：

```toml
[[providers]]
name = "my-openai"
channel = "openai"

[providers.settings]
base_url = "https://api.openai.com"

[[providers.credentials]]
secret = "sk-your-openai-key-here"
```

然后启动（或重启）GPROXY：

```bash
./gproxy
```

首次运行时，GPROXY 读取 `gproxy.toml`，将所有内容写入数据库，并生成管理员凭据。后续运行从数据库加载，忽略 TOML 文件（数据已存在）。

### 方式 C：多提供商 TOML

```toml
[[providers]]
name = "openai-main"
channel = "openai"

[providers.settings]
base_url = "https://api.openai.com"

[[providers.credentials]]
secret = "sk-key-1"

[[providers.credentials]]
secret = "sk-key-2"

[[providers]]
name = "claude"
channel = "anthropic"

[providers.settings]
base_url = "https://api.anthropic.com"

[[providers.credentials]]
secret = "sk-ant-your-key"
```

同一提供商的多个凭据会通过健康感知选择进行负载均衡。如果某个 key 触发速率限制，GPROXY 自动切换到下一个。

## 7. 发送请求

无作用域（GPROXY 根据请求体中的模型名称路由）：

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer <api-key>" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}'
```

指定提供商（显式指定目标提供商）：

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer <api-key>" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}'
```
