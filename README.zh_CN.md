# GPROXY

让 OpenAI、Anthropic 和 Gemini 兼容客户端通过同一个网关访问不同上游。GPROXY 负责
Provider 路由、协议转换、凭据、配额和可观测性，并提供内嵌控制台处理日常管理。可以部署为
原生二进制、Docker 容器或 Serverless Edge 函数。

[English](README.md) · 简体中文

[![GitHub Sponsors](https://img.shields.io/github/sponsors/LeenHawk?logo=githubsponsors&label=赞助)](https://github.com/sponsors/LeenHawk)

- 🪪 **许可证：** AGPL-3.0-or-later · 🐳 **镜像：** `ghcr.io/leenhawk/gproxy`
- 🦀 **构建目标：** 原生二进制 · Docker · Edge Wasm（Cloudflare / Deno / Netlify / Supabase / EdgeOne / Appwrite）
- 🖥️ **控制台：** 内置，路径 `/console`

---

## 它做什么

GPROXY 为应用提供稳定的统一 API，上游 Provider 可以按需选择和组合：

- **多供应商路由** —— OpenAI、Anthropic、Gemini/Vertex、DeepSeek、Groq、OpenRouter、
  NVIDIA、Vercel AI Gateway、Claude Code、Codex、Grok Build，以及任意 OpenAI 兼容自定义端点。
- **两种路由模式** —— 聚合 `/v1/...`（Provider 写在模型名里）与 Scoped
  `/{provider}/v1/...`（Provider 写在 URL 里）。
- **跨协议转换** —— OpenAI 客户端可以使用 Claude 或 Gemini 上游，响应会再转换回客户端
  需要的格式。
- **多租户鉴权** —— 用户、API key、glob 模型权限、RPM/RPD/token 限速和 USD 配额。
- **提示词与请求控制** —— Claude 和 OpenAI 缓存断点、可复用改写规则、凭据故障转移和熔断。
- **可插拔存储** —— SQLite / PostgreSQL / MySQL，可选静态加密。
- **内置控制台** —— 无需单独部署前端。

---

## 部署

### 🐳 Docker（推荐）

完全自包含：内嵌控制台、本地文件存储、无需外部服务。

[![Deploy to Koyeb](https://www.koyeb.com/static/images/deploy/button.svg)](https://app.koyeb.com/deploy?type=docker&image=ghcr.io/leenhawk/gproxy&ports=8787;http;/&name=gproxy&env[GPROXY_ADMIN_PASSWORD]=change-me)
[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/LeenHawk/gproxy)

```bash
docker run -p 8787:8787 -e GPROXY_ADMIN_PASSWORD=change-me ghcr.io/leenhawk/gproxy
# 然后打开 http://localhost:8787/console (admin / change-me)
```

> 对外提供服务前请设置自己的管理员密码。`GPROXY_ADMIN_PASSWORD` 为空或只有空白字符时，
> 容器会拒绝启动。
>
> **明文 HTTP 访问控制台** 在同站部署下可用，包括局域网 IP、服务器 IP 和隧道。
> 将 GPROXY 暴露到本地开发之外时建议使用 HTTPS；跨站 console 部署仍需要 HTTPS cookie。

### ☁️ Serverless Edge（WebAssembly）

六个边缘平台的预构建产物都在
[**`deploy` 分支**](https://github.com/LeenHawk/gproxy/tree/deploy)，部署时不需要准备 Rust
工具链。Edge 部署使用 **Turso** 保存持久配置，也可以选用 **Upstash** 做共享缓存。各平台的
具体步骤见[边缘部署指南](https://gproxy.leenhawk.com/zh-cn/deployment/edge/)。

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https://github.com/LeenHawk/gproxy&branch=deploy&create_from_path=netlify)

Cloudflare 和 Netlify 按钮会在部署前要求填写必需的 `TURSO_URL` 和
`TURSO_TOKEN` secrets。这里要填 Turso 的 HTTP URL（`https://<db>.turso.io`），
不要填 `libsql://` URL。可选的 Upstash cache 和 `GPROXY_MASTER_KEY` secrets
可以在 worker/site 创建后再补。Cloudflare Workers、Netlify Edge、Deno Deploy 和
EdgeOne Pages 会在同一个部署里带上 Console 静态资产。设置 `GPROXY_ADMIN_USER` 和
非空的 `GPROXY_ADMIN_PASSWORD`，部署后打开 `/console`。

| 平台 | 产物 | 部署 |
|---|---|---|
| Cloudflare Workers | [`deploy/cloudflare`](https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare) | 部署按钮或 `wrangler deploy` |
| Netlify Edge | [`deploy/netlify`](https://github.com/LeenHawk/gproxy/tree/deploy/netlify) | 部署按钮或 `netlify deploy --prod` |
| Deno Deploy | — | `deploy/deno/build.sh`（CLI） |
| Supabase Edge | [`deploy/supabase`](https://github.com/LeenHawk/gproxy/tree/deploy/supabase) | `supabase functions deploy gproxy`（Docker/eszip，CLI） |
| EdgeOne Pages | [`deploy/eopages`](https://github.com/LeenHawk/gproxy/tree/deploy/eopages) | `edgeone pages deploy`（CLI） |
| **Appwrite Functions** | [`deploy/appwrite-deno`](https://github.com/LeenHawk/gproxy/tree/deploy/appwrite-deno) | `appwrite push functions`（deno-2.0，CLI） |

### 📦 原生安装包与二进制

请优先前往 **[下载页](https://gproxy.leenhawk.com/zh-cn/getting-started/downloads/)**
或[最新 GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest)。Release 提供
Android APK、Windows MSI、macOS DMG、Linux DEB 和便携 ZIP。Linux GNU 与 musl
版本支持 x86_64、AArch64 和 RISC-V 64，两套 Docker 镜像也发布相同的三种架构。
桌面安装版会在后台运行，并可在 Console 设置中开关自动启动。

> 如果只是想使用 GPROXY，请下载预构建产物。如果不是要开发 GPROXY 或制作定制版本，
> 请不要克隆仓库并自行编译。

---

## 配置

环境变量用于配置进程本身。Provider、凭据、路由、用户等运行期设置保存在数据库中，并通过
`/console` 管理。

| 变量 | 默认 | 用途 |
|---|---|---|
| `GPROXY_HOST` / `GPROXY_PORT` | `127.0.0.1` / `8787` | 监听地址 |
| `GPROXY_PERSISTENCE` | 二进制：`db`；Docker：`file` | `db` 使用 SQLite/PostgreSQL/MySQL；`file` 按表保存 JSON，仅适合单实例 |
| `GPROXY_DSN` | 自动生成 SQLite DSN | `persistence=db` 时可选的 PostgreSQL/MySQL/SQLite DSN |
| `GPROXY_MASTER_KEY` | — | 解封存储的密文（缺省时明文存储） |
| `GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` | `admin` / 随机 | 首启动管理员 |

**从 v1 升级？** 让 v2 使用现有 SQLite 数据库即可。首次启动时，GPROXY 会导入受支持的
配置，并把旧数据库保留为 `*.v1.bak` 备份。

---

## 第一个请求

```bash
# 聚合 —— 供应商/模型写在 body 里
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer <your-key>" -H "Content-Type: application/json" \
  -d '{"model":"openai-main/gpt-4.1-mini","messages":[{"role":"user","content":"Hello"}]}'
```

运维端点（`/healthz`、`/version`、`/metrics`）需要 Admin 鉴权。

## 文档

- **[下载](https://gproxy.leenhawk.com/zh-cn/getting-started/downloads/)**
- **[文档首页](https://gproxy.leenhawk.com/zh-cn/)**
- **[快速开始](https://gproxy.leenhawk.com/zh-cn/getting-started/quick-start/)**
- **[提示缓存](https://gproxy.leenhawk.com/zh-cn/guides/claude-caching/)**
- **[边缘部署](https://gproxy.leenhawk.com/zh-cn/deployment/edge/)**
- **[新增 Channel](https://gproxy.leenhawk.com/zh-cn/guides/adding-a-channel/)**

## Star 趋势

[![Star History 趋势图](https://api.star-history.com/svg?repos=LeenHawk/gproxy&type=Date)](https://www.star-history.com/#LeenHawk/gproxy&Date)

## 支持项目

如果 GPROXY 对你有帮助，可以通过 [GitHub Sponsors](https://github.com/sponsors/LeenHawk)
支持项目持续开发。

## 许可证

[AGPL-3.0-or-later](LICENSE) · 作者：[LeenHawk](https://github.com/LeenHawk)
