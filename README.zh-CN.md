<div align="center">

# GPROXY

**把不同的大模型服务接到同一个 API 入口。**

[English](README.md) · [简体中文](README.zh-CN.md)

[![Release](https://img.shields.io/github/v/release/LeenHawk/gproxy)](https://github.com/LeenHawk/gproxy/releases/latest)
[![CI](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml/badge.svg)](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue)](LICENSE)
[![Stars](https://img.shields.io/github/stars/LeenHawk/gproxy?style=flat)](https://github.com/LeenHawk/gproxy/stargazers)

[文档](https://gproxy.leenhawk.com/zh-cn/) · [下载](https://github.com/LeenHawk/gproxy/releases) · [讨论](https://github.com/LeenHawk/gproxy/discussions) · [赞助](https://github.com/sponsors/LeenHawk)

[![Documentation](https://img.shields.io/badge/文档-gproxy.leenhawk.com-2563eb?style=for-the-badge)](https://gproxy.leenhawk.com/zh-cn/)

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-button)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless)
[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY&products=%5B%7B%22type%22%3A%22integration%22%2C%22group%22%3A%22postgres%22%2C%22protocol%22%3A%22storage%22%7D%5D)
[![Deploy to Deno](https://img.shields.io/badge/Deploy_to-Deno-000000?style=for-the-badge&logo=deno)](https://console.deno.com/new?clone=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&path=deploy%2Fserverless)

</div>

GPROXY 是用 Rust 编写、可自行部署的 LLM API 网关。把上游账户接进来，在控制台配置模型路由，客户端就可以用同一组地址和网关 API Key 调用不同服务。切换供应商、更新凭证、查看用量和费用，都在网关里完成。

v4 提供带设置向导的桌面与移动应用，也可以作为命令行服务、容器运行，或部署到 Cloudflare、Netlify、Vercel 和 Deno。正式使用下载[最新稳定版](https://github.com/LeenHawk/gproxy/releases/latest)；开发快照见 [nightly](https://github.com/LeenHawk/gproxy/releases/tag/nightly)。

## 帮 GPROXY 上架应用商店

- [ ] Google Play 开发者账号：$25（一次性）
- [ ] Apple 开发者计划：$99/年

上架后可以直接从手机应用商店安装，并自动更新。[通过 GitHub Sponsors 赞助 →](https://github.com/sponsors/LeenHawk)

## 能做什么

<table>
<tr>
<td width="50%" valign="top">
<h3>协议与客户端</h3>
<p>接受 OpenAI Chat Completions、Responses、Claude Messages 和 Gemini GenerateContent，支持协议转换和流式响应。Codex CLI、Claude Code 等客户端可按指南接入。</p>
</td>
<td width="50%" valign="top">
<h3>上游账户池</h3>
<p>按渠道使用 API Key、OAuth 或 Cookie。集中管理凭证刷新、健康状态和额度；凭证失效或上游限流时，按配置尝试其他凭证或供应商。</p>
</td>
</tr>
<tr>
<td valign="top">
<h3>模型路由</h3>
<p>给客户端一个固定模型名，例如 <code>fast</code>。在网关中调整对应的供应商、上游模型、权重和回退层级，无需逐个修改客户端。</p>
</td>
<td valign="top">
<h3>请求改写</h3>
<p>为供应商配置系统提示词、缓存断点、JSON 改写、正则替换和请求头规则，在接入不同客户端时调整请求格式。</p>
</td>
</tr>
<tr>
<td valign="top">
<h3>权限、用量与费用</h3>
<p>管理用户、组织、团队和 API Key，设置访问权限与费用预算。控制台可查看用量、上下游请求日志和审计记录。</p>
</td>
<td valign="top">
<h3>本地或服务器部署</h3>
<p>个人使用可选带界面的应用，服务器可用 CLI 或容器，也可部署到 Cloudflare、Netlify、Vercel 和 Deno。Rust 项目可通过 SDK 嵌入网关能力。</p>
</td>
</tr>
</table>

内置渠道包括 OpenAI、Claude API / Code / Web、Codex、Gemini CLI、Google AI Studio、Copilot、DeepSeek、GLM、MiniMax、Kimi、OpenRouter、AWS Bedrock、Azure、Vertex 和 xAI 等。兼容 OpenAI、Claude 或 Gemini 的服务也可以通过 `custom` 渠道接入。

图片、音频、文件、WebSocket / Realtime 等操作的支持范围取决于渠道与上游，详见[供应商与凭证](https://gproxy.leenhawk.com/zh-cn/guides/providers/)和[客户端指南](https://gproxy.leenhawk.com/zh-cn/guides/cli-clients/)。

## 界面预览

以下为 v4 控制台实拍，使用独立演示实例和示例数据。

**供应商与模型** — 在同一页面管理上游账户和模型目录。

![供应商与模型管理](docs/images/readme/providers-zh-CN.webp)

<details>
<summary>查看模型路由与请求规则</summary>

**模型路由** — 为固定模型名配置供应商、权重和回退层级。

![模型路由管理](docs/images/readme/routes-zh-CN.webp)

**请求规则** — 为供应商设置系统提示词和缓存断点。

![请求规则配置](docs/images/readme/rules-zh-CN.webp)

</details>

## 快速开始

### 在电脑或手机上使用

1. 从 [Releases](https://github.com/LeenHawk/gproxy/releases/latest) 下载对应平台的 **Application**（`gproxy-tauri-*`）。
2. 启动应用，按向导配置实例、管理员账户和可选的配置导入。
3. 保存服务地址和网关 API Key，进入控制台添加供应商与上游凭证。
4. 测试凭证，创建名为 `fast` 的模型路由，把测试成功的模型添加为成员。

应用自带控制台，其 HTTP 端口供客户端调用网关 API。

### 在服务器上运行

下载 **CLI**（`gproxy-*`），解压后运行：

```sh
chmod +x ./gproxy
./gproxy serve --data-dir ./data --port 8787
```

Windows 使用 `gproxy.exe`。首次启动会在终端显示管理员密码和 API Key，请保存，然后打开 **http://127.0.0.1:8787/console/**。添加供应商、测试凭证并创建模型路由的步骤与应用版相同。

默认只监听本机，SQLite 数据库保存在 `./data/gproxy.db`。在添加凭证前配置并保存 `GPROXY_MASTER_KEY`，否则上游密钥以明文存储。更新时保留数据目录和主密钥；远程访问请配置监听地址与 HTTPS。参数见[配置参考](https://gproxy.leenhawk.com/zh-cn/reference/configuration/)。

### 发出第一个请求

将下面的 Key 替换为网关生成的 API Key。`fast` 是上一步创建的路由名：

```sh
export GPROXY_KEY='your-gproxy-api-key'
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"你好"}],"stream":true}'
```

### 托管平台一键部署

Cloudflare、Netlify、Vercel 和 Deno 的模板使用 **latest 稳定版** 预编译包，无需编译 Rust。Cloudflare 可选 D1 或 libSQL/Turso；其他三个平台使用 PostgreSQL。填写数据库连接、管理员密码和主密钥后部署，再打开 `/console/` 登录。

部署入口、平台数据库与外部数据库的选择，以及 WebSocket 等能力差异，统一见[托管平台部署指南](https://gproxy.leenhawk.com/zh-cn/deployment/edge/)。

以下三个模板使用 **latest 稳定版** PostgreSQL 预编译程序与轻量容器。部署前请核对应用和数据库的付费套餐。

<table>
<tr>
<td width="33%" valign="top">
<h3>Render</h3>
<p>Starter 服务 + Basic PostgreSQL，自动绑定数据库并生成密码与主密钥。</p>
<p><a href="https://render.com/deploy?repo=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev"><strong>部署到 Render →</strong></a></p>
</td>
<td width="33%" valign="top">
<h3>Heroku</h3>
<p>Basic dyno + Essential PostgreSQL，填写主密钥后自动创建应用和数据库。</p>
<p><a href="https://heroku.com/deploy?template=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev"><strong>部署到 Heroku →</strong></a></p>
</td>
<td width="33%" valign="top">
<h3>Northflank</h3>
<p>导入模板即可创建项目、PostgreSQL 和服务；目前需手动导入，尚无公开分享链接。</p>
<p><a href="https://gproxy.leenhawk.com/zh-cn/deployment/containers/#northflank"><strong>导入模板 →</strong></a></p>
</td>
</tr>
</table>

[初始化与运行说明](https://gproxy.leenhawk.com/zh-cn/deployment/containers/)

### 其他部署方式

| 方式 | 适用场景 | 入口 |
| --- | --- | --- |
| Application | 桌面或移动设备，应用内管理 | [平台安装说明](https://gproxy.leenhawk.com/zh-cn/getting-started/installation/) |
| CLI / 容器 | 常驻服务器，浏览器管理 | [安装与容器配置](https://gproxy.leenhawk.com/zh-cn/getting-started/installation/) |
| Cloudflare / Netlify / Vercel / Deno | 托管平台，无需自备服务器 | [部署指南](https://gproxy.leenhawk.com/zh-cn/deployment/edge/) |
| Northflank / Render / Heroku | 原生容器与托管 PostgreSQL | [部署指南](https://gproxy.leenhawk.com/zh-cn/deployment/containers/) |
| Rust SDK | 嵌入自己的程序 | [gproxy-sdk](crates/gproxy-sdk/README.zh-CN.md) |

Windows 可从 Microsoft Store 安装两个版本：

| GPROXY Desktop | GPROXY CLI |
| :---: | :---: |
| <a href="https://apps.microsoft.com/detail/9P2FJRB9RS4Z?mode=direct"><img src="https://get.microsoft.com/images/zh-cn%20dark.svg" alt="从 Microsoft 获取：GPROXY Desktop" height="48"></a> | <a href="https://apps.microsoft.com/detail/9NBMH3S5K0L9?mode=direct"><img src="https://get.microsoft.com/images/zh-cn%20dark.svg" alt="从 Microsoft 获取：GPROXY CLI" height="48"></a> |

Release 中的 MSIX 是商店提交包，直接安装请选 ZIP；macOS 应用尚未公证；鸿蒙 HAP 已完成模拟器验证，安装前需要自行签名。各平台要求见安装说明。

## 性能

配置加载为内存快照，出站客户端按连接配置复用。日志采集可以按需关闭，正文记录单独设置。

以下使用官方 **[v4.0.0 Linux x86_64 release](https://github.com/LeenHawk/gproxy/releases/tag/v4.0.0)** 实测。Ryzen 7 8745H（8 核 16 线程）、NVMe SQLite，压测客户端、网关和 mock 上游运行在同一台机器。请求为非流式 Chat Completions，经过鉴权、模型路由、用量提取、计价和落库；上游固定返回 25 个输入 token、18 个输出 token，不额外添加延迟。

每轮先为各日志配置预热 2 秒，再使用 `oha 1.16.0` 对各并发数测量 10 秒，共三轮，表中各指标取三轮中位数。正文日志关闭；两种日志配置均开启用量记录和费用结算。

| 请求日志 | 并发连接 | 请求/秒 | p50 | p99 |
| --- | ---: | ---: | ---: | ---: |
| 关闭 | 1 | 2,882 | 0.34 ms | 0.50 ms |
| 关闭 | 32 | 28,303 | 0.76 ms | 2.71 ms |
| 关闭 | 64 | 27,263 | 1.47 ms | 41.91 ms |
| 上下游元数据 | 64 | 7,765 | 2.81 ms | 45.16 ms |

单连接下，相比直连 mock，网关增加的中位延迟约 **0.30 ms**。测量阶段共完成 **1,989,395** 次请求，全部 HTTP 200；等待写入完成后，用量记录数与请求数一致，token 数和费用逐行校验通过。每个请求按输入、输出各 1 美元 / 百万 token 计价，记录费用为 0.000043 美元。

64 并发时，吞吐没有继续上升，p99 增至约 42 ms；开启元数据日志后约为 45 ms。本轮只覆盖短响应、同协议转发和本机 SQLite，长流式响应、协议转换、远程数据库及真实上游需要按实际负载另测。

## 从 v3 升级

停止 v3，备份数据库、主密钥与启动配置，再用原配置启动 v4。支持的 v3 SQLite、PostgreSQL、MySQL 和 D1 数据库会自动迁移，保留账户、密码、API Key 和历史用量；无法映射的配置会在报告中列出。不要让 v3 与 v4 同时写入同一数据库。

迁移范围、备份与失败处理见[从 v3 迁移到 v4](https://gproxy.leenhawk.com/zh-cn/deployment/v3-to-v4/)。

## CI 无前端版

Release 工作流独立构建以下无前端 ZIP 包，无需等待前端构建：

| 平台 | 架构 | 下载包 |
| --- | --- | --- |
| Linux GNU | x86_64、aarch64、riscv64 | `gproxy-headless-linux-<arch>.zip` |
| Linux musl | x86_64、aarch64、riscv64 | `gproxy-headless-linux-<arch>-musl.zip` |
| Windows | x86_64、aarch64 | `gproxy-headless-windows-<arch>.zip` |
| macOS | x86_64、aarch64 | `gproxy-headless-macos-<arch>.zip` |
| Android（Termux） | x86_64、aarch64 | `gproxy-headless-android-<arch>.zip` |

在 [Releases](https://github.com/LeenHawk/gproxy/releases) 下载：开发版选 `nightly`，
beta 选 `staging`，稳定版选正式版本。自更新也会选择对应的无前端包。
Android 先在 Termux 执行 `pkg install libc++ openssl ca-certificates`，
将 ZIP 解压到 Termux 主目录，再运行 `./gproxy serve --console=false`。
Windows 使用 `gproxy.exe`。

此版本不打包 Web 控制台，保留代理、管理 API、全部渠道、SQLite、内存缓存、
本地文件存储和内置词表，通过 CLI 或管理 API 配置。源码构建不需要 Node.js、
pnpm 或桌面库：

```sh
cargo build --locked --release -p gproxy --bin gproxy \
  --no-default-features --features channels,memory,fs,bundled-vocabulary
./target/release/gproxy serve --console=false
```

不启用 `embedded-console` 时，即使工作目录已有前端产物，也不会嵌入二进制。
`--console=false` 同时关闭外部控制台目录的服务。自定义 feature 构建若需要
内嵌控制台，添加 `embedded-console`。

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

桌面和 WASM 的独立检查见[从源码构建](https://gproxy.leenhawk.com/zh-cn/deployment/release-build/)。工作区结构见[架构](https://gproxy.leenhawk.com/zh-cn/introduction/architecture/)，扩展方式见[新增渠道](https://gproxy.leenhawk.com/zh-cn/guides/adding-a-channel/)。

问题请提交到 [Issues](https://github.com/LeenHawk/gproxy/issues)，不便公开的内容也可以发邮件到 [leenhawk@leenhawk.com](mailto:leenhawk@leenhawk.com)。安全漏洞请通过 [Security](https://github.com/LeenHawk/gproxy/security) 或邮件私下报告，详见 [SECURITY.md](SECURITY.md)。

## 许可

网关应用使用 **AGPL-3.0-or-later**，见 [LICENSE](LICENSE)。`gproxy-protocol`、`gproxy-protocol-macros`、`gproxy-client`、`gproxy-cache`、`gproxy-file`、`gproxy-seaorm` 和 `gproxy-tokenizer` 使用 **MIT**，各自目录包含许可证。

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=LeenHawk/gproxy&type=Date)](https://www.star-history.com/#LeenHawk/gproxy&Date)
