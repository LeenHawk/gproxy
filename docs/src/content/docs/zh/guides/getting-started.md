---
title: 快速开始
description: 从零启动 GPROXY 并完成首个请求验证。
---

本页提供最小可用启动流程，目标是尽快跑通一个可用的代理实例。

## 1. 准备依赖

去[release](https://github.com/LeenHawk/gproxy/releases)下载对应平台的二进制文件

## 2. 准备配置文件

可以参考[完整可配置文件](https://github.com/LeenHawk/gproxy/blob/main/gproxy.example.full.toml)

最小可用配置示例：

```toml
[global]
host = "127.0.0.1"
port = 8787
proxy = ""
admin_key = "replace-with-strong-admin-key"
mask_sensitive_info = true
data_dir = "./data"
dsn = "sqlite://./data/gproxy.db?mode=rwc"

[[channels]]
id = "openai"
enabled = true

[channels.settings]
base_url = "https://api.openai.com"

[[channels.credentials]]
secret = "sk-replace-me"
```

## 3. 启动服务

直接运行下载后的二进制：

```bash
# Linux / macOS
./gproxy
```

```bash
# Windows
gproxy.exe
```

启动后可在日志中看到：

- 监听地址（默认 `http://127.0.0.1:8787`）
- 当前 admin key（`password:`）

如果 `gproxy.toml` 不存在，服务会以内存默认配置启动并自动生成 16 位 admin key。

## 4. 发送最小验证请求

```bash
curl -sS http://127.0.0.1:8787/v1/models \
  -H "x-api-key: <你的 user key 或 admin key>"
```

如果只想用某个渠道的模型的话

```bash
curl -sS http://127.0.0.1:8787/claudecode/v1/models \
  -H "x-api-key: <your user key or admin key>"
```

如果返回模型列表 JSON，说明路由、鉴权和上游连接已打通。

## 5. 打开管理后台

- 控制台入口：`GET /`
- 默认静态资源路径：`/assets/*`
