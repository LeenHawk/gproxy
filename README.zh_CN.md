# gproxy

**单个 Rust 二进制的高性能、多供应商 LLM 代理** —— 内嵌 React 控制台、多租户鉴权,
同一套引擎可**原生 / Docker / 边缘(WebAssembly)** 三种形态运行。

[English](README.md) · 简体中文

- 🪪 **许可证:** AGPL-3.0-or-later · 🐳 **镜像:** `ghcr.io/leenhawk/gproxy`
- 🦀 **构建目标:** 原生二进制 · Docker · 边缘 wasm(Cloudflare / Deno / Netlify / Supabase / EdgeOne / Appwrite)
- 🖥️ **控制台:** 内置,路径 `/console`

---

## 它做什么

gproxy 在众多上游 LLM 供应商之上,暴露统一的 **OpenAI / Anthropic / Gemini 兼容**
HTTP 接口,并补齐把它当共享服务运行所需的一切:

- **多供应商路由** —— OpenAI、Anthropic、Gemini/Vertex、DeepSeek、Groq、OpenRouter、
  NVIDIA、Vercel AI Gateway、Claude Code、Codex,以及任意 OpenAI 兼容自定义端点。
- **两种路由模式** —— 聚合 `/v1/...`(供应商写在模型名里)与限定 `/{provider}/v1/...`
  (供应商写在 URL 里)。
- **跨协议转换** —— OpenAI 客户端可打 Claude 上游(反之亦然);同方言走极简解析快路径。
- **多租户鉴权** —— 用户、API key、glob 模型权限、RPM/RPD/token 限速、USD 配额;
  Claude 提示缓存、改写规则、熔断。
- **可插拔存储** —— SQLite / PostgreSQL / MySQL,可选静态加密。
- **内置控制台** —— 无需单独部署前端。

---

## 部署

### 🐳 一键(Docker,推荐)

完全自包含:内嵌控制台、文件式 SQLite、无需外部服务。

[![Deploy to Koyeb](https://www.koyeb.com/static/images/deploy/button.svg)](https://app.koyeb.com/deploy?type=docker&image=ghcr.io/leenhawk/gproxy&ports=8787;http;/&name=gproxy&env[GPROXY_ADMIN_PASSWORD]=change-me)
[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/LeenHawk/gproxy)

```bash
docker run -p 8787:8787 -e GPROXY_ADMIN_PASSWORD=change-me ghcr.io/leenhawk/gproxy
# 然后打开 http://localhost:8787/console (admin / change-me)
```

### ☁️ Serverless 边缘(WebAssembly)

同一套路由可作为 wasm 边缘函数跑在六个平台上。**预构建、即点即部署**的产物在
[**`deploy` 分支**](https://github.com/LeenHawk/gproxy/tree/deploy)(平台侧没有 cargo,
无需工具链)。边缘函数需要外部 **Turso** 控制面库(+ 可选 **Upstash** 缓存);完整步骤见
**[docs/edge-deploy.md](docs/edge-deploy.md)**。

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https://github.com/LeenHawk/gproxy&branch=deploy&create_from_path=netlify)

| 平台 | 产物 | 部署 |
|---|---|---|
| Cloudflare Workers | [`deploy/cloudflare`](https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare) | 一键按钮 ☝️ / `wrangler deploy` |
| Netlify Edge | [`deploy/netlify`](https://github.com/LeenHawk/gproxy/tree/deploy/netlify) | 一键按钮 ☝️ / `netlify deploy --prod` |
| Deno Deploy | — | `deploy/deno/build.sh`(CLI) |
| Supabase Edge | [`deploy/supabase`](https://github.com/LeenHawk/gproxy/tree/deploy/supabase) | `supabase functions deploy gproxy`(Docker/eszip,CLI) |
| EdgeOne Pages | [`deploy/eopages`](https://github.com/LeenHawk/gproxy/tree/deploy/eopages) | `edgeone pages deploy`(CLI) |
| **Appwrite Functions** | [`deploy/appwrite-deno`](https://github.com/LeenHawk/gproxy/tree/deploy/appwrite-deno) | `appwrite push functions`(deno-2.0,CLI) |

### 📦 原生二进制

每个 [release](https://github.com/LeenHawk/gproxy/releases) 都提供预编译二进制
(linux/macOS/windows,x86_64 + aarch64)。或自行 `cargo build --release`。

---

## 配置

gproxy 用**环境变量**配置;运行期配置进数据库,通过 `/console` 管理。

| 变量 | 默认 | 用途 |
|---|---|---|
| `GPROXY_HOST` / `GPROXY_PORT` | `127.0.0.1` / `8787` | 监听地址 |
| `GPROXY_PERSISTENCE` | `file` | `file`(`GPROXY_DATA_DIR` 下的 SQLite)或 `db` |
| `GPROXY_DSN` | — | `persistence=db` 时的 DSN(Postgres/MySQL/SQLite) |
| `GPROXY_MASTER_KEY` | — | 解封存储的密文(缺省=明文) |
| `GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` | `admin` / 随机 | 首启动管理员 |

**从 v1 升级?** 把 v2 二进制指向你现有的 v1 SQLite 库,首启动会就地迁移(旧库备份为
`*.v1.bak`)。

---

## 第一个请求

```bash
# 聚合 —— 供应商/模型写在 body 里
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer <your-key>" -H "Content-Type: application/json" \
  -d '{"model":"openai-main/gpt-4.1-mini","messages":[{"role":"user","content":"Hello"}]}'
```

运维端点(`/healthz`、`/version`、`/metrics`)走 admin 鉴权。

## 文档

- **[边缘部署](docs/edge-deploy.md)** · **[架构设计](docs/architecture-design.md)** · **[开发者指南](docs/developers/README.md)**

## 许可证

[AGPL-3.0-or-later](LICENSE) · 作者:[LeenHawk](https://github.com/LeenHawk)
