# Console 部署形态

gproxy v2 的 `console/` 是一份 Vite 构建出来的静态 SPA，默认挂在
`/console/`。它不是单独的后端服务；页面里的 API 请求使用相对路径，例如
`/admin/*`、`/user/*`、`/healthz`、`/version`、`/metrics`。因此最稳妥的部署模型是：
**console 静态资源和 gproxy API 对浏览器保持同源**。

后端 HTTP 面分为四类：

| 路径 | 用途 | 鉴权 |
| --- | --- | --- |
| `/console/*` | 管理/门户 SPA 静态资源 | 无，页面内再登录 |
| `/admin/*` | 管理 API、登录、CRUD、观测、自更新 | admin session 或 admin API key |
| `/user/*` | 用户门户 API | session |
| `/v1/*`、`/{provider}/v1/*`、`/v1beta/*` | LLM 网关 | user API key |
| `/healthz`、`/version`、`/metrics` | 运维端点 | admin |

## 构建产物

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
```

`pnpm build` 会执行三步：

1. `tsc -b` 类型检查。
2. `vite build` 生成 `console/dist/`，base path 为 `/console/`。
3. `node scripts/sync-to-embed.mjs` 同步 `dist/` 到 `../assets/console/`。

`assets/console/` 是 native 二进制的嵌入目录，由 `rust-embed` 编进最终二进制。
这个目录是构建产物，不应该手工维护。

## 形态 0：内嵌到 native 二进制

这是默认生产形态。先构建 console，再构建 Rust 二进制：

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..

cargo build --release
```

运行 native 实例后：

- `/` 永久重定向到 `/console`；
- `/console`、`/console/` 和 `/console/<route>` 返回 SPA；
- `assets/` 下的 hash 静态文件使用长期缓存；
- `index.html` 使用 `no-cache`；
- SPA 与 API 完全同源，不需要 CORS。

如果没有构建 console，`assets/console/` 只有占位文件，`/console` 会返回明确的
`console assets not embedded` 错误；这不影响后端编译或网关 API。

适用场景：单机、Docker、普通 VM、需要最少部署部件的生产环境。

## 形态 1：独立静态托管 + 同源反代

如果你不想把 console 编进二进制，可以把 `console/dist/` 放到 Nginx、Caddy、
S3+CDN 或其它静态资源系统，但浏览器看到的域名仍应把 API 路径反代回 gproxy。

示意：

```text
https://gproxy.example.com/console/*  -> static dist/
https://gproxy.example.com/admin/*    -> gproxy native/edge API
https://gproxy.example.com/user/*     -> gproxy native/edge API
https://gproxy.example.com/v1/*       -> gproxy gateway
https://gproxy.example.com/healthz    -> gproxy ops endpoint
https://gproxy.example.com/version    -> gproxy ops endpoint
https://gproxy.example.com/metrics    -> gproxy ops endpoint
```

这个形态仍然是同源部署，cookie、CSRF、`fetch(..., { credentials: "include" })`
都按默认浏览器规则工作，不需要设置 `GPROXY_CORS_ORIGINS`。

注意：当前 Vite 配置的 base path 是 `/console/`。如果要把 SPA 挂在站点根路径，
需要修改 `console/vite.config.ts` 的 `base` 后重新构建。

## 形态 2：跨域 API

后端支持显式 CORS 白名单：

```bash
GPROXY_CORS_ORIGINS=https://console.example.com,https://ops.example.com
```

启用后，native `/admin/*` 和 `/user/*` router 会：

- 只允许白名单里的完整 origin，不能使用 `*`；
- 允许 credentialed CORS；
- 允许 `content-type`、`authorization`、`x-api-key` 请求头；
- 让 session cookie 使用跨站场景需要的 `SameSite=None; Secure`；
- 将白名单 origin 纳入 CSRF 放行逻辑。

但当前 console 前端的 `api()` 使用相对路径，没有内置 API base URL 配置。也就是说，
把 `dist/` 直接放到 `https://console.example.com/console/` 时，浏览器会请求
`https://console.example.com/admin/*`，不会自动去请求 `https://api.example.com/admin/*`。

因此跨域能力主要适用于：

- 自定义 console 构建或外壳，把 API path 改成绝对后端 URL；
- 静态托管层把 `/admin`、`/user` 等路径转发到后端，同时保留浏览器 Origin；
- 非 console 的浏览器客户端。

普通 console 部署优先选择内嵌或同源反代。

## 形态 3：edge 同源静态资源

edge wasm worker 可以服务网关、`/admin/*` 和 `/user/*`。推荐做法是让 edge 平台同时
托管 console 静态资源，并保持同一域名：

```text
https://edge.example.com/console/*  -> platform static assets
https://edge.example.com/admin/*    -> gproxy wasm worker
https://edge.example.com/user/*     -> gproxy wasm worker
https://edge.example.com/v1/*       -> gproxy wasm worker
```

edge 入口在 `src/http/edge/` 中直接按 path 分发，不走 native Axum router。它的控制面
使用 libSQL/Turso 持久化，缓存使用 Upstash 或 libSQL KV。`init()` 由平台胶水代码传入
Turso、Upstash 和可选 `GPROXY_MASTER_KEY`。

edge 管理面当前有三类显式降级：

| 端点 | edge 行为 | 原因 |
| --- | --- | --- |
| `/admin/update/*` | 501 `not_implemented` | 自更新只适用于 native 二进制。 |
| `/admin/login-flows/cookie` | 501 `not_implemented` | Claude Code cookie 登录依赖 native wreq/TLS 行为。 |
| `/admin/credentials/{id}/usage` | 501 `not_implemented` | 实时上游用量查询依赖 native 路径。 |

其它控制面和门户路径应通过 edge dispatcher 提供。详见 `docs/edge-deploy.md`。

## 开发模式

后端：

```bash
GPROXY_INSECURE_COOKIES=1 cargo run --features full
```

前端：

```bash
cd console
pnpm install --frozen-lockfile
pnpm dev
```

当前 `console/vite.config.ts` 代理 `/admin`、`/healthz`、`/version` 和 `/metrics`
到 `http://127.0.0.1:8787`，并把 Origin 改写成后端地址以通过 CSRF 检查。

如果你要在 Vite dev server 中测试 `/user/*` 门户路径，需要同步给 Vite 添加 `/user`
代理，或者改用内嵌 console 路径测试门户功能。

## 部署选择

| 形态 | 静态资源位置 | API 是否同源 | 是否需要 CORS | 适用场景 |
| --- | --- | --- | --- | --- |
| 内嵌 native | `assets/console/` 编进二进制 | 是 | 否 | 默认生产部署、Docker、VM。 |
| 同源反代 | 外部静态托管 | 是 | 否 | 前端资产独立发布，但域名统一。 |
| 跨域 API | 外部静态托管或自定义前端 | 否 | 是 | 自定义 console/API base 或其它浏览器客户端。 |
| edge 同源 | edge 平台静态资源 | 是 | 否 | 纯 edge 部署，共享 Turso/Upstash。 |

生产环境优先使用“内嵌 native”或“同源反代”。这两个形态和当前 console 的相对 API
路径完全匹配，cookie 与 CSRF 规则也最简单。

## English

# Console Deployment Shapes

The gproxy v2 `console/` is a static SPA built by Vite and mounted at
`/console/` by default. It is not a separate backend service. API calls inside
the SPA use relative paths such as `/admin/*`, `/user/*`, `/healthz`,
`/version`, and `/metrics`. The safest deployment model is therefore:
**serve the console static assets and the gproxy API from the same browser
origin**.

The backend HTTP surface has four groups:

| Path | Purpose | Auth |
| --- | --- | --- |
| `/console/*` | Admin/user portal SPA assets | None; the page logs in later |
| `/admin/*` | Admin API, login, CRUD, observability, self-update | Admin session or admin API key |
| `/user/*` | User portal API | Session |
| `/v1/*`, `/{provider}/v1/*`, `/v1beta/*` | LLM gateway | User API key |
| `/healthz`, `/version`, `/metrics` | Ops endpoints | Admin |

## Build Output

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
```

`pnpm build` runs three steps:

1. `tsc -b` type checking.
2. `vite build`, producing `console/dist/` with `/console/` as the base path.
3. `node scripts/sync-to-embed.mjs`, syncing `dist/` into `../assets/console/`.

`assets/console/` is the native binary embed directory compiled through
`rust-embed`. It is build output and should not be maintained by hand.

## Shape 0: Embedded In The Native Binary

This is the default production shape. Build the console before building the Rust
binary:

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..

cargo build --release
```

After the native instance starts:

- `/` permanently redirects to `/console`;
- `/console`, `/console/`, and `/console/<route>` return the SPA;
- hash-named static files under `assets/` use long-lived caching;
- `index.html` uses `no-cache`;
- the SPA and API are same-origin, so CORS is unnecessary.

If the console has not been built, `assets/console/` contains only placeholders
and `/console` returns a clear `console assets not embedded` error. Backend
compilation and gateway APIs still work.

Use this for single-node installs, Docker, VMs, and production deployments that
should have the fewest moving parts.

## Shape 1: Separate Static Hosting With Same-Origin Reverse Proxy

If you do not want to embed the console in the binary, place `console/dist/` in
Nginx, Caddy, S3+CDN, or another static asset system, but keep the browser-facing
domain routing API paths back to gproxy.

Example:

```text
https://gproxy.example.com/console/*  -> static dist/
https://gproxy.example.com/admin/*    -> gproxy native/edge API
https://gproxy.example.com/user/*     -> gproxy native/edge API
https://gproxy.example.com/v1/*       -> gproxy gateway
https://gproxy.example.com/healthz    -> gproxy ops endpoint
https://gproxy.example.com/version    -> gproxy ops endpoint
https://gproxy.example.com/metrics    -> gproxy ops endpoint
```

This remains a same-origin deployment. Cookies, CSRF, and
`fetch(..., { credentials: "include" })` follow normal browser rules and do not
need `GPROXY_CORS_ORIGINS`.

The current Vite base path is `/console/`. To mount the SPA at site root, change
`console/vite.config.ts` and rebuild.

## Shape 2: Cross-Origin API

The backend supports an explicit CORS allow-list:

```bash
GPROXY_CORS_ORIGINS=https://console.example.com,https://ops.example.com
```

When enabled, the native `/admin/*` and `/user/*` routers:

- allow only the exact listed origins, never `*`;
- allow credentialed CORS;
- allow `content-type`, `authorization`, and `x-api-key`;
- use `SameSite=None; Secure` session cookies for cross-site sessions;
- include allowed origins in CSRF checks.

The current console frontend still uses relative paths in `api()`. If you serve
`dist/` directly from `https://console.example.com/console/`, the browser will
request `https://console.example.com/admin/*`; it will not automatically call
`https://api.example.com/admin/*`.

Cross-origin support is mainly for:

- a custom console build or shell with an absolute API base URL;
- a static hosting layer that forwards `/admin`, `/user`, and related paths to
  the backend while keeping the browser Origin;
- non-console browser clients.

For the normal console, prefer embedded native or same-origin reverse proxy.

## Shape 3: Edge Same-Origin Static Assets

The edge wasm worker can serve gateway traffic, `/admin/*`, and `/user/*`.
Recommended deployment keeps console static assets on the same edge domain:

```text
https://edge.example.com/console/*  -> platform static assets
https://edge.example.com/admin/*    -> gproxy wasm worker
https://edge.example.com/user/*     -> gproxy wasm worker
https://edge.example.com/v1/*       -> gproxy wasm worker
```

The edge entry in `src/http/edge/` dispatches directly by path and does not run
the native Axum router. Its control plane uses libSQL/Turso persistence and
Upstash or libSQL KV for cache. Platform glue calls `init()` with Turso,
Upstash, and optional `GPROXY_MASTER_KEY` values.

The edge admin surface currently has three explicit downgrades:

| Endpoint | Edge behavior | Reason |
| --- | --- | --- |
| `/admin/update/*` | 501 `not_implemented` | Self-update only applies to the native binary. |
| `/admin/login-flows/cookie` | 501 `not_implemented` | Claude Code cookie login depends on native wreq/TLS behavior. |
| `/admin/credentials/{id}/usage` | 501 `not_implemented` | Live upstream usage fetch depends on the native path. |

Other control-plane and portal paths should be served through the edge
dispatcher. See `docs/edge-deploy.md`.

## Development Mode

Backend:

```bash
GPROXY_INSECURE_COOKIES=1 cargo run --features full
```

Frontend:

```bash
cd console
pnpm install --frozen-lockfile
pnpm dev
```

`console/vite.config.ts` currently proxies `/admin`, `/healthz`, `/version`, and
`/metrics` to `http://127.0.0.1:8787`, rewriting Origin to satisfy CSRF checks.

To test `/user/*` portal paths through the Vite dev server, add a `/user` proxy
there as well, or test through the embedded console path.

## Choosing A Deployment Shape

| Shape | Static asset location | API same-origin | Needs CORS | Use case |
| --- | --- | --- | --- | --- |
| Embedded native | `assets/console/` compiled into the binary | Yes | No | Default production, Docker, VM. |
| Same-origin reverse proxy | External static hosting | Yes | No | Independent frontend asset release with one domain. |
| Cross-origin API | External hosting or custom frontend | No | Yes | Custom console/API base or other browser clients. |
| Edge same-origin | Edge platform static assets | Yes | No | Pure edge deployment with shared Turso/Upstash. |

Production should usually choose embedded native or same-origin reverse proxy.
Both match the current console's relative API paths and keep cookie/CSRF behavior
simple.
