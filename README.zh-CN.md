# GPROXY

[English](README.md) | [简体中文](README.zh-CN.md) · [文档](https://gproxy.leenhawk.com/zh-cn/) · [下载](https://github.com/LeenHawk/gproxy/releases) · [讨论](https://github.com/LeenHawk/gproxy/discussions)

[![CI](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml/badge.svg)](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/LeenHawk/gproxy)](LICENSE)

**用一个入口连接你的大模型服务、上游账户和应用。**

GPROXY 是可自行部署的 LLM API 网关：统一管理上游凭证，路由和转换请求，执行权限、
限流与费用配额，并记录用量和账单。原生部署只需一个可执行文件，同时提供 API、
管理员控制台和用户 Portal。

## 能做什么

- **使用你习惯的客户端。** 接受 OpenAI Chat Completions、OpenAI Responses、
  Claude Messages 和 Gemini GenerateContent，支持流式响应和格式间直接转换；
  其他操作的支持范围取决于所选渠道。
- **管理上游账户池。** 按渠道支持 API Key、OAuth 或 Cookie，提供令牌刷新、
  健康状态、凭证选择和故障切换。
- **保持模型名称稳定。** 用“公开模型名 → 路由 → 供应商／上游模型”的模型映射，
  在切换供应商时避免逐个修改客户端配置。
- **控制权限和费用。** 管理用户、组织、团队、权限、限流、费用配额和多维计价；
  包括 CLI 兼容入口在内的推理请求都经过统一准入与结算流程。
- **在界面中完成配置。** 控制台管理供应商、凭证、模型目录、规则集、用量、
  配额历史和更新；用户 Portal 管理账户、API Key 和 OAuth 授权会话。
- **选择合适的部署方式。** 提供原生程序和安装包、容器、Android 包及预构建 Edge
  包；Rust 应用也可嵌入不依赖 HTTP 服务框架或 UI 的 `gproxy-core`。

渠道包括 OpenAI、Claude API / Claude Code / Claude Web、Gemini CLI、Codex、
Copilot、OpenRouter、AWS Bedrock、Vertex、Azure、Kimi 等。
认证方式和不同运行环境的支持范围见[供应商文档](https://gproxy.leenhawk.com/zh-cn/guides/providers/)。

## 快速开始

### 原生程序

在 [Releases](https://github.com/LeenHawk/gproxy/releases) 下载适合系统的安装包或
便携压缩包。Linux / macOS 解压后运行：

```sh
chmod +x ./gproxy
./gproxy
```

Windows 运行 `gproxy.exe`。打开 **http://127.0.0.1:8787/admin** 创建首个管理员，
用户 Portal 位于 **/portal**。

原生程序默认只监听本机，数据库保存在 `./data/gproxy.db`。更新程序时保留数据目录。

### 容器

```sh
docker run -d --name gproxy --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v gproxy-data:/app/data \
  ghcr.io/leenhawk/gproxy:v3.0.0
```

发布镜像使用 **65532:65532** 用户运行，数据目录是 **/app/data**。命名卷可保留数据；
绑定宿主机目录时需给该用户写权限。镜像覆盖 amd64、arm64、riscv64，并提供
`-musl` 变体。只有需要滚动开发版本时才使用 `:staging`。

更多方式见[容器部署](https://gproxy.leenhawk.com/zh-cn/deployment/docker/)和
[Edge 部署](https://gproxy.leenhawk.com/zh-cn/deployment/edge/)。

### 发送第一个请求

1. 在控制台添加供应商，填入或登录授权上游凭证。
2. 拉取模型目录，配置要对外使用的模型和路由。
3. 为用户授予访问权限，创建 API Key。
4. 在终端设置 `GPROXY_API_KEY`，把示例中的 `my-model` 替换成控制台或
   `GET /v1/models` 显示的可访问模型 ID。

```sh
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"my-model","messages":[{"role":"user","content":"Hello"}],"stream":true}'
```

应用只需要网关地址、GPROXY API Key 和模型 ID。
其他格式见[第一个请求](https://gproxy.leenhawk.com/zh-cn/getting-started/first-request/)。

## CLI 客户端和 Pi

Codex CLI、Claude Code 可使用 GPROXY 的兼容服务入口。
请按[客户端指南](https://gproxy.leenhawk.com/zh-cn/guides/cli-clients/)配置对应的供应商路径和认证方式。

Pi 使用独立、MIT 许可的 [pi-gproxy](https://github.com/LeenHawk/pi-gproxy) 扩展：

```sh
pi install npm:pi-gproxy
```

在 **控制台 → 设置 → OAuth 客户端** 启用 `pi-gproxy`，然后在 Pi 输入 `/login`，
选择 **GPROXY**。浏览器 PKCE 和设备码登录授权的是网关用户账户，不是上游账户。
扩展会发现该账户可用的模型，推理仍走正常网关流程。

Portal 的**授权会话**显示成功登录次数、仍有效会话和刷新次数，撤销后相应令牌失效。
Pi 的本地 `/logout` 不等于服务器撤销。接口说明见[账户 OAuth](docs/account-oauth.md)。

## 配置与安全

配置依次读取命令行参数、环境变量、工作目录和数据目录中的 `.env`。
运行 `gproxy --help` 可查看完整原生参数。

| 环境变量 | 用途 |
| --- | --- |
| `GPROXY_HOST`、`GPROXY_PORT` | 监听地址；原生默认 `127.0.0.1:8787`。 |
| `GPROXY_DATA_DIR` | 本地持久化目录；原生默认 `./data`，发布容器默认 `/app/data`。 |
| `GPROXY_PERSISTENCE` | `sqlite`、`libsql`、`postgres` 或 `mysql`。 |
| `GPROXY_DSN` | 所选数据库需要的连接字符串。 |
| `GPROXY_MASTER_KEY` | 可选的标准 Base64 编码 32 字节密钥，用于加密存储的敏感信息。 |
| `GPROXY_UPSTREAM_PROXY_URL` | 默认上游代理覆盖。 |

未设置主密钥时，凭证和 API Key 以明文存储，请保护数据目录和备份。
启用加密后要妥善保存密钥，不要每次重启都重新生成；更换密钥必须按轮换流程操作。
远程访问应放在 HTTPS 后面，并明确配置可信代理和允许的来源。

加密、存储、缓存、初始化和代理配置见[配置参考](https://gproxy.leenhawk.com/zh-cn/reference/configuration/)。

## 从 v2 升级

**升级前备份 v2 程序、数据库和启动配置。**

v3 使用不同的数据模型。原生启动可以备份并迁移受支持的 v2 SQLite 数据库，
保留可恢复的密钥、受支持的配置和用量，并在校验通过后原子切换数据库。

遇到未覆盖的非空数据表、按路由限定的权限或其他无法转换的数据时，自动迁移会停止，
不会静默丢弃。现有 v2 更新器不保留旧程序，因此二进制回滚需要事先保存的程序，
或对应的官方 v2 包。远程数据库需要显式迁移方案。

请先阅读[升级与回滚指南](docs/v2-upgrade.md)。
`main` 现在维护 v3，v2 源码仍可通过版本标签和 Git 历史获取。

## 开发

Rust 工作区分为可嵌入核心、渠道、成对协议转换、共享存储、应用服务和原生／Edge 宿主。
React 控制台位于 `console/`，文档位于 `docs/`。
管理 API 的 TypeScript 类型由 Rust 经 `cargo test` 生成，不手工维护镜像类型。

```sh
cargo run -p gproxy-host-axum
pnpm --dir console install --frozen-lockfile
pnpm --dir console dev
```

提交改动前运行：

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
pnpm --dir console lint
pnpm --dir console test
pnpm --dir console build
```

扩展开发见[架构](https://gproxy.leenhawk.com/zh-cn/introduction/architecture/)和
[添加渠道](https://gproxy.leenhawk.com/zh-cn/guides/adding-a-channel/)。
一般问题请提交 [Issue](https://github.com/LeenHawk/gproxy/issues)，漏洞请通过
[Security](https://github.com/LeenHawk/gproxy/security) 私下报告。

## 许可证

网关应用采用 **AGPL-3.0-or-later**，见 [LICENSE](LICENSE)。
部分可复用协议／转换 crate 采用 **MIT**，以各自的 `Cargo.toml` 为准；
独立 Pi 扩展同样采用 MIT。
