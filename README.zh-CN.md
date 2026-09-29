# GPROXY

[English](README.md) | 简体中文 · [文档](https://gproxy.leenhawk.com/zh-cn/) · [下载](https://github.com/LeenHawk/gproxy/releases) · [讨论](https://github.com/LeenHawk/gproxy/discussions)

**一个入口，连接你的大模型服务。**

GPROXY 是可自行部署的 LLM API 网关。它统一管理上游账户，支持 OpenAI、Claude 和 Gemini 协议转换、模型路由、凭证故障切换、访问控制，以及用量和费用记录。

本分支面向 **4.0.0**，正式版正在准备中。试用 v4 可选择 Releases 中的 `nightly`；正式版上线后，请选择对应版本的附件。

## 快速开始

1. 在 [Releases](https://github.com/LeenHawk/gproxy/releases) 下载适合系统的 **Application**（`gproxy-tauri-*`）。
2. 启动应用，按三步向导配置连接、管理员账户和可选的配置导入。
3. 保存服务地址和网关 API Key，进入控制台添加供应商与上游凭证。
4. 测试凭证，创建名为 `fast` 的模型路由，把测试成功的模型添加为成员。

```sh
export GPROXY_KEY='your-gproxy-api-key'
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"你好"}]}'
```

服务器部署选择 **CLI**（`gproxy-*`），解压后运行：

```sh
./gproxy serve --data-dir ./data --port 8787
```

Windows 使用 `gproxy.exe`。首次启动会在终端显示管理员密码和 API Key，请保存；浏览器控制台位于 **http://127.0.0.1:8787/console/**。Application 使用应用内控制台，其 HTTP 端口仅供网关 API 调用。

CLI 默认使用 SQLite，数据保存在 `./data/gproxy.db`。在添加凭证前配置并保存 `GPROXY_MASTER_KEY`，否则上游密钥以明文存储。更新时保留数据目录和主密钥。

平台安装要求、签名限制、容器与 Workers 部署见[安装文档](https://gproxy.leenhawk.com/zh-cn/getting-started/installation/)。Windows MSIX 附件是未签名商店提交包；macOS 应用尚未公证；鸿蒙 HAP 为实验性产物。

## 常用功能

- **上游账户池**：按渠道支持 API Key、OAuth 或 Cookie，管理凭证健康状态、刷新和故障切换。
- **模型路由**：客户端使用固定模型名，管理员配置供应商、上游模型、权重和回退层级。
- **协议转换**：支持主要生成协议及流式响应，其他操作的支持范围取决于渠道和上游。
- **重写规则**：配置系统提示词、缓存断点、JSON 改写、正则替换和请求头。
- **用户与费用**：管理用户、组织、团队、API Key、权限和费用预算，查看用量、请求和审计记录。
- **多种部署方式**：CLI、桌面与移动应用、容器、Cloudflare Workers；Rust 应用可嵌入核心库。

进一步阅读：[快速开始](https://gproxy.leenhawk.com/zh-cn/getting-started/quick-start/) · [供应商与凭证](https://gproxy.leenhawk.com/zh-cn/guides/providers/) · [CLI 客户端](https://gproxy.leenhawk.com/zh-cn/guides/cli-clients/)

## 从 v3 升级

停止 v3，备份数据库、主密钥与启动配置，再用原配置启动 v4。支持的 v3 SQLite 数据库会自动迁移，保留账户、密码、API Key 和历史用量；无法映射的配置会在报告中列出。不要让 v3 与 v4 同时写入同一数据库。

迁移范围、备份与失败处理见[从 v3 迁移到 v4](https://gproxy.leenhawk.com/zh-cn/deployment/v3-to-v4/)。

## 开发

原生构建需要 stable Rust、Go 和 Clang；控制台与文档需要 Node.js 22.12+（推荐 24 LTS）和 pnpm。Linux 桌面构建还需要 WebKitGTK 4.1、GTK 3 与 libsoup 3 开发包。

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build
cargo run -p gproxy -- serve
```

控制台构建会同步嵌入资源。只运行 `cargo build` 的全新检出不包含控制台。

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm --dir console lint
pnpm --dir console test
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs check
pnpm --dir docs build
```

桌面和 WASM 的独立检查见[从源码构建](https://gproxy.leenhawk.com/zh-cn/deployment/release-build/)。架构与扩展方式见[架构](https://gproxy.leenhawk.com/zh-cn/introduction/architecture/)和[新增渠道](https://gproxy.leenhawk.com/zh-cn/guides/adding-a-channel/)。

问题请提交到 [Issues](https://github.com/LeenHawk/gproxy/issues)，安全漏洞请通过 [Security](https://github.com/LeenHawk/gproxy/security) 私下报告。

## 许可

网关应用使用 **AGPL-3.0-or-later**，见 [LICENSE](LICENSE)。`gproxy-protocol`、`gproxy-protocol-macros`、`gproxy-client`、`gproxy-cache`、`gproxy-file`、`gproxy-seaorm` 和 `gproxy-tokenizer` 使用 **MIT**，各自目录包含许可证。
