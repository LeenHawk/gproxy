# GPROXY

**使用 Rust 编写的高性能 LLM 代理服务器。** 多供应商、多租户，内嵌
React 控制台 —— 全部塞进一个静态二进制里。

- 📘 **文档：** <https://gproxy.leenhawk.com/zh-cn/>
- 📦 **下载：** <https://gproxy.leenhawk.com/zh-cn/downloads/>
- 🦀 **Crate：** `gproxy-sdk`
- 🪪 **许可证：** AGPL-3.0-or-later
- 🌐 **语言：** 简体中文 · [English](./README.md)

---

## 能做什么

GPROXY 在多家上游 LLM 供应商之上暴露一个统一的、**OpenAI / Anthropic /
Gemini 兼容**的 HTTP 接口，并提供把它作为共享服务运行所需的一切基础设施：

- **多供应商路由** —— OpenAI、Anthropic、Vertex / Gemini、DeepSeek、
  Groq、OpenRouter、NVIDIA、Claude Code、Codex、Antigravity，以及任意
  OpenAI 兼容的自定义端点。
- **两种路由模式** —— 聚合 `/v1/...`（供应商名编码在 model 字段里）
  和限定作用域 `/{provider}/v1/...`（供应商在 URL 里）。
- **同协议透传** —— 当客户端和上游使用相同协议时，走最小解析的快路径。
- **跨协议翻译** —— OpenAI 客户端可以路由到 Claude 上游（反之亦然），
  通过协议 `transform` 层完成请求和响应格式转换。
- **多租户鉴权** —— 用户、API key、glob 模型权限、RPM / RPD / token
  限流、美元配额。
- **Claude 提示缓存** —— `anthropic` / `claudecode` 通道的服务端
  `cache_breakpoint` 规则和魔法字符串触发器。
- **请求改写 & 消息改写规则** —— 对请求体任意 JSON 字段的操作，以及
  对消息文本内容的正则替换。
- **内嵌 React 控制台** —— 编译进二进制，挂载在 `/console`，无需单独
  部署前端。
- **可插拔存储** —— SQLite / PostgreSQL / MySQL（通过 SeaORM / SQLx），
  可选的 XChaCha20-Poly1305 磁盘加密。
- **Rust SDK** —— `gproxy-sdk` 再导出协议、路由、供应商三个 crate，
  方便你把引擎嵌入到自己的服务中。

## 快速开始

```bash
# 1. 构建
git clone https://github.com/LeenHawk/gproxy.git
cd gproxy
cargo build -p gproxy --release

# 2. 用最小配置运行
GPROXY_CONFIG=./gproxy.toml ./target/release/gproxy
```

一份最小的 `gproxy.toml` 种子配置，会创建一个带通配权限的管理员用户：

> **注意：** `gproxy.toml` 只会在**数据库不存在**的首次启动时被读取
> **一次**。种子导入完成后，数据库就是唯一的真相源 —— 之后再修改
> `gproxy.toml` 不会生效。日常配置请通过 `/console` 控制台（或管理
> API）进行；如需重新用 TOML 重新种子，请先删除数据库文件。

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
credentials = [ { api_key = "sk-your-upstream-key" } ]

[[models]]
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true

[[users]]
name = "admin"
password = "change-me"
is_admin = true
enabled = true

[[users.keys]]
api_key = "sk-admin-1"
label = "default"
enabled = true

[[permissions]]
user_name = "admin"
model_pattern = "*"
```

然后打开 <http://127.0.0.1:8787/console>，用 `admin` 登录。

完整流程见 **[快速开始](https://gproxy.leenhawk.com/zh-cn/getting-started/quick-start/)**。

## 发送第一个请求

```bash
# 聚合入口 —— 在 body 里写 provider/model 前缀
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai-main/gpt-4.1-mini",
    "messages": [ { "role": "user", "content": "你好" } ]
  }'

# 限定作用域入口 —— 供应商在 URL 里，body 里只放上游原始 id
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [ { "role": "user", "content": "你好" } ]
  }'
```

Anthropic 和 Gemini 的例子见
**[发送第一个请求](https://gproxy.leenhawk.com/zh-cn/getting-started/first-request/)**。

## 仓库结构

```text
apps/                  # 可运行二进制
  gproxy/              # 主二进制 (HTTP 服务 + 内嵌控制台)
  gproxy-recorder/     # 上游流量录制工具 (开发/调试)
crates/                # 主程序组合使用的服务端 crate
  gproxy-core/         # 配置、身份、策略、配额、路由类型
  gproxy-storage/      # SeaORM 存储 + 静态加密 + schema 同步
  gproxy-api/          # 管理与用户 HTTP API、鉴权、登录、CORS
  gproxy-server/       # 把上述部分组装在一起的 Axum 服务
sdk/                   # 框架无关库 (不依赖 DB/HTTP)
  gproxy-protocol/     # L0：OpenAI/Claude/Gemini 协议类型 + transform
  gproxy-channel/      # L1：Channel trait、各通道实现、凭证、计费、
                       #     工具函数、健康状态
  gproxy-engine/       # L2：GproxyEngine、ProviderStore、路由、
                       #     重试、凭证亲和、后端 trait
  gproxy-sdk/          # 重导出上述三层的伞 crate
frontend/console/      # React 控制台，构建时嵌入到二进制
docs/                  # Starlight 文档站 (gproxy.leenhawk.com 的源)
```

## 文档

完整文档在 **<https://gproxy.leenhawk.com/zh-cn/>**，常用入口：

- [GPROXY 是什么?](https://gproxy.leenhawk.com/zh-cn/introduction/what-is-gproxy/)
- [架构概览](https://gproxy.leenhawk.com/zh-cn/introduction/architecture/)
- [安装](https://gproxy.leenhawk.com/zh-cn/getting-started/installation/)
- [供应商与通道](https://gproxy.leenhawk.com/zh-cn/guides/providers/)
- [模型与别名](https://gproxy.leenhawk.com/zh-cn/guides/models/)
- [权限、限流与配额](https://gproxy.leenhawk.com/zh-cn/guides/permissions/)
- [请求改写规则](https://gproxy.leenhawk.com/zh-cn/guides/rewrite-rules/) · [消息改写规则](https://gproxy.leenhawk.com/zh-cn/guides/message-rewrite/)
- [Claude 提示缓存](https://gproxy.leenhawk.com/zh-cn/guides/claude-caching/)
- [新增通道](https://gproxy.leenhawk.com/zh-cn/guides/adding-a-channel/)
- [路由表](https://gproxy.leenhawk.com/zh-cn/reference/routing-table/)
- [环境变量](https://gproxy.leenhawk.com/zh-cn/reference/environment-variables/) · [TOML 配置](https://gproxy.leenhawk.com/zh-cn/reference/toml-config/)
- [Rust SDK](https://gproxy.leenhawk.com/zh-cn/reference/sdk/)

本地预览文档：

```bash
cd docs
pnpm install
pnpm dev
```

## 许可证

以 [AGPL-3.0-or-later](./LICENSE) 许可证发布。

作者：[LeenHawk](https://github.com/LeenHawk)
