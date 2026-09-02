---
title: "Edge Wasm"
description: "把 gproxy edge bundle 部署到 Cloudflare Workers、Deno Deploy 或 Netlify Edge，使用 libSQL 数据库和可选的 Upstash 缓存"
---

`crates/gproxy-host-edge` 把与原生二进制相同的应用核心编译为
`wasm32-unknown-unknown`，运行在基于 fetch 的平台上。每个平台一个简短的 TypeScript
入口：读取绑定，构造 `EdgeConfig`，每个 isolate 调用一次 `start()`，再把所有非静态
请求交给 `EdgeHost.fetch(request, clientIp)`。管理 API、门户 API、聚合模式的
`/v1/...` 路径和按名称指定 Provider 的路径行为与原生一致；差异见「限制」一节。
各平台源码位于 `deploy/cloudflare`、`deploy/deno` 和 `deploy/netlify`。

## 绑定

| 绑定 | 必需 | 用途 |
| --- | --- | --- |
| `GPROXY_LIBSQL_URL` | 是 | libSQL 数据库的绝对 `https://` URL。store 通过 HTTP 上的 Hrana 通信；`libsql://` URL 会被拒绝。 |
| `GPROXY_LIBSQL_AUTH_TOKEN` | 是 | 数据库 auth token。 |
| `GPROXY_MASTER_KEY` | 否 | 标准 base64，32 字节。用 AES-256-GCM 加密保存凭证和用户密钥；不设置则为明文。 |
| `GPROXY_MASTER_KEY_NEXT` | 否 | 轮换目标；空值表示轮换回明文。 |
| `GPROXY_MASTER_KEY_ROTATE` | 否 | `1`、`true`、`yes` 或 `on` 为一次部署启用轮换。 |
| `UPSTASH_URL`、`UPSTASH_TOKEN` | 否 | Upstash REST 缓存。`EdgeConfig` 的构造函数把它们作为最后两个参数接收，但随附的入口没有传入；要使用 Upstash 需自行扩展入口。 |

把这些值保存为平台 secret。edge 上不读取 `.env` 文件，`GPROXY_ADMIN_*` 首次运行
变量也仅原生可用。

Turso 是常见的 libSQL 提供方：创建数据库和 token，使用数据库的 HTTPS URL
（`https://<db>-<org>.turso.io`）。第一个请求到达时，store 会创建 schema 并写入
全局价格目录。保存配额、限流、刷新租约和准入状态的缓存是同一数据库中的一张表，
因此每个 isolate 看到的状态一致。见[存储与缓存后端](/zh-cn/reference/database/)。

## 静态层与 Rust

| 路径 | 由谁提供 |
| --- | --- |
| `/`、`/admin`、`/admin/**`（`/admin/api/**` 除外）、`/portal`、`/portal/` | 静态 `index.html`（控制台 SPA），仅 `GET`/`HEAD` |
| `/assets/**`、`/favicon.svg` | 静态控制台资源 |
| `/admin/api/**`、`/portal/api/**` | Rust：管理与门户分发 |
| 其余全部 | Rust：网关入口（`/v1/...`、Claude 与 Gemini 原生路径、按名称指定 Provider 的路径、WebSocket 升级） |

Cloudflare 的 `wrangler.toml` 设置了 `run_worker_first = true`，因此 Worker 会看到
每个请求，并把静态请求转给 `ASSETS` 绑定，同时把 `/admin/*` 重写为 `/`。Deno 的
`main.ts` 从 `public/` 读取同样的文件。Netlify 通过 `netlify.toml` 中的
`excludedPath` 把静态路径留在 CDN 上，并把 `/admin/*` 重写为 `/`。交给 Rust 的
客户端 IP 分别来自 `cf-connecting-ip`、`remoteAddr.hostname` 和 `Context.ip`。
请求体上限为 100 MiB，与原生相同。

## WebSocket

| 平台 | 升级 | 机制 |
| --- | --- | --- |
| Cloudflare Workers | 支持 | `WebSocketPair`；用 `ExecutionContext.waitUntil` 保持转发存活 |
| Deno Deploy | 支持 | `Deno.upgradeWebSocket`；`main.ts` 保留 continuation 直到结束 |
| Netlify Edge | 不支持 | 没有升级 API。Rust 返回 `501`，正文为 `websocket upgrades are unavailable in this fetch runtime` |

## 预构建 Bundle

每个 release 发布 `gproxy-edge-cloudflare.zip`、`gproxy-edge-deno.zip`、
`gproxy-edge-netlify.zip` 和原始的 `gproxy-edge.wasm`，各带 `.sha256`，另有
`gproxy-edge.provenance.json`。zip 解压为 `<platform>/`，内含入口文件、配置、
`pkg/`（wasm 与 wasm-bindgen glue）和 `public/`（控制台构建产物）。GitHub 的
`releases/latest` 不包含预发布版本，因此 v3 处于 alpha 期间请从带版本号的 release
页面下载；见[下载](/zh-cn/getting-started/downloads/)。

`wrangler.toml` 与 `netlify.toml` 都声明了 `[build] command = "pnpm run build"`，它会
用 `wasm-pack` 从源码编译。部署预构建 bundle 时，删除该段，避免平台尝试源码构建。

### Cloudflare Workers

```sh
cd cloudflare
pnpm install
pnpm exec wrangler secret put GPROXY_LIBSQL_URL
pnpm exec wrangler secret put GPROXY_LIBSQL_AUTH_TOKEN
pnpm exec wrangler deploy
```

`wrangler.toml` 把 `src/index.ts` 指定为 Worker，`./public` 作为静态资源目录并
绑定为 `ASSETS`，compatibility date 为 `2026-08-26`。`pnpm run dev` 在本地运行
`wrangler dev`。

### Deno Deploy

bundle 由 `main.ts`、`deno.json`、`pkg/` 和 `public/` 组成。把项目入口指向
`main.ts`，并把绑定设为项目环境变量。本地运行 `deno task start`，即
`deno run --allow-net --allow-env=<五个 GPROXY_* 变量名> --allow-read=./pkg,./public main.ts`。

### Netlify Edge

`netlify.toml` 发布 `public/`，并在 `/*`（减去静态路径）上注册 `gproxy` Edge
Function。把 `GPROXY_LIBSQL_URL` 和 `GPROXY_LIBSQL_AUTH_TOKEN` 设为敏感站点变量，然后：

```sh
cd netlify
pnpm install
pnpm run deploy   # netlify deploy --prod
```

## 自行构建 Bundle

需要带 `wasm32-unknown-unknown` target 的 Rust、`wasm-pack`、Node.js LTS 与 pnpm，
构建 Deno bundle 还需要 Deno。

```sh
cd deploy/cloudflare && pnpm install && pnpm run build && pnpm run check
cd deploy/netlify && pnpm install && pnpm run build && pnpm run check
cd deploy/deno && deno task build && deno task check
```

`build:wasm` 运行 `wasm-pack build ../../crates/gproxy-host-edge --release`，Cloudflare
用 `--target bundler`，Deno 与 Netlify 用 `--target web`，输出到 `pkg/`。`build:assets`
构建控制台并把 `console/dist` 复制到 `public/`。两个目录都被 gitignore。
`scripts/package-edge-release.sh` 用一次 `cargo build` 产出全部三个 zip；它需要
预构建的 `console/dist` 和与 `Cargo.lock` 匹配的 `wasm-bindgen` CLI。见
[构建与发布](/zh-cn/deployment/release-build/)。

## 首次启动

打开 `https://<your-deployment>/admin`。store 为空时 `GET /admin/api/session` 返回
`setup_required: true`，控制台显示初始化表单；`POST /admin/api/setup` 创建第一个
管理员并完成登录。之后的流程与原生相同：添加 Provider，粘贴或登录凭证，创建路由，
签发用户密钥；见[快速开始](/zh-cn/getting-started/quick-start/)。`/portal` 对用户
同样可用。

## 限制

| 原生功能 | 在 edge 上 |
| --- | --- |
| Claude Web 通道 | 未编译进 wasm 构建 |
| Provider 或凭证的代理覆盖 | 请求失败，提示 `configured upstream proxy is unavailable in the fetch runtime` |
| TLS/HTTP2 指纹覆盖 | 请求失败，提示 `configured TLS/HTTP2 fingerprint is unavailable in the fetch runtime` |
| 默认出口代理（`GPROXY_UPSTREAM_PROXY_URL`） | 无；出口就是平台的 `fetch` |
| 连通性探测 | `400`，提示 `connectivity testing is unavailable on edge` |
| 分词器词表下载、Hugging Face token | `403`；内置分词器梯度仍可计数 |
| 自更新、自动启动、公告 | 不存在；它们是原生 host 的路由 |
| SQLite、PostgreSQL、MySQL、Redis | 不可用；仅 libSQL，Upstash 可选 |
| Netlify 上的 WebSocket | `501` |

每个 isolate 启动自己的 host，并从 libSQL 加载自己的快照；isolate 之间不共享内存，
libSQL 缓存表（或 Upstash）是它们之间配额与限流保持一致的唯一依据。
