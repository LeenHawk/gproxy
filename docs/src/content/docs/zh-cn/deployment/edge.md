---
title: Edge Wasm 部署
description: 将预构建的 GPROXY v2 WebAssembly bundle 部署到支持的 edge 平台。
---

edge runtime 是同一个单 Rust crate 用 `--no-default-features --features edge` 编译出的
`wasm32-unknown-unknown` library。平台入口负责加载 wasm-bindgen glue，调用 Rust
`init(...)` 建立 `AppState`，并把每个请求转交给 wasm `fetch` 路径。

不要依赖 edge 平台从源码编译 Rust。使用 release 或 `deploy` branch 中的预构建
bundle，或在 CI 中构建 bundle 后上传生成产物。

## 一键部署

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https://github.com/LeenHawk/gproxy&branch=deploy&create_from_path=netlify)

这些按钮使用 `deploy` branch 的预构建产物。把下面的 runtime 服务配置为平台 secrets。

Cloudflare 模板在 `deploy/cloudflare/.dev.vars.example` 中声明了必需的 Turso
secrets，Netlify 模板在 `deploy/netlify/netlify.toml` 中声明。两个一键部署流程都会在
第一次部署前要求填写 `TURSO_URL` 和 `TURSO_TOKEN`。这里要填 Turso 的 HTTP URL
（`https://<db>.turso.io`），不要填 `libsql://` URL，因为 edge runtime 通过 `fetch`
调用 Hrana。它们也会要求填写 `GPROXY_ADMIN_USER` 和 `GPROXY_ADMIN_PASSWORD`；密码至少
12 个字符，用于第一次登录 Console。如果部署需要 Upstash cache 或 sealed stored secrets，
可以之后再添加可选的 `UPSTASH_URL`、`UPSTASH_TOKEN` 和 `GPROXY_MASTER_KEY` secrets。

Cloudflare Workers、Netlify Edge、Deno Deploy 和 EdgeOne Pages 会把 React Console
放进同一个部署，并让 `/` 跳到 `/console/`。Supabase Edge Functions 和 Appwrite
Functions 默认是平台函数 URL，不是站点根路径；如果这些平台也需要 Web UI，需要通过自定义
同源根路径/反代来服务 Console。

## Runtime 服务

edge runtime 不能直连本地 SQLite、PostgreSQL、MySQL 或 Redis。v2 edge 使用 HTTP 可访问服务：

| 变量 | 必需 | 用途 |
| --- | --- | --- |
| `TURSO_URL` | 是 | libSQL/Turso HTTP URL，例如 `https://<db>.turso.io`。 |
| `TURSO_TOKEN` | 是 | Turso access token。 |
| `GPROXY_ADMIN_USER` | 是 | Console 登录用的 admin 用户名。 |
| `GPROXY_ADMIN_PASSWORD` | 是 | Console 登录用的 admin 密码；至少 12 个字符。 |
| `UPSTASH_URL` | 否 | Upstash Redis cache；缺省时回退到 libSQL KV。 |
| `UPSTASH_TOKEN` | 否 | Upstash token。 |
| `GPROXY_MASTER_KEY` | 否 | 标准 base64 32 字节 sealed secret key。 |

这些值应放在平台 secret 或环境变量系统中，不要写进 bundle。

## 预构建 Bundle

release workflow 发布：

| Artifact | 目标 |
| --- | --- |
| `gproxy-edge-cloudflare.zip` | Cloudflare Workers。 |
| `gproxy-edge-netlify.zip` | Netlify Edge Functions。 |
| `gproxy-edge-supabase.zip` | Supabase Edge Functions。 |
| `gproxy-edge-deno.zip` | Deno Deploy compact upload root。 |
| `gproxy-edge-eopages.zip` | Tencent EdgeOne Pages。 |
| `gproxy-edge-appwrite-deno.zip` | Appwrite Functions on `deno-2.0`。 |
| `gproxy.wasm` | 供检查或自定义打包的 raw wasm。 |

发布 release 时，workflow 还会刷新 orphan `deploy` branch，里面只包含 ready-to-deploy
artifacts：wasm、glue、平台入口和 config，不包含源码构建流程。

## 本地 Bundle 构建

本地构建适合验证或临时 artifact：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
```

`wasm-bindgen-cli` 必须匹配 `Cargo.lock` 中的 `wasm-bindgen` crate 版本。当前 workflow
安装 `0.2.123`。

生成平台 bundle：

```bash
bash deploy/cloudflare/build.sh
bash deploy/netlify/build.sh
bash deploy/supabase/build.sh
bash deploy/eopages/build.sh
bash deploy/appwrite-deno/build.sh
```

`deploy/deno/build.sh` 不同：它会通过 Deno Deploy CLI module 构建并部署，所以 release
workflow 不直接调用该脚本，而是内联生成 Deno bundle。

## 平台形态

| 平台组 | Bundle 形态 |
| --- | --- |
| Cloudflare Workers | `wasm-bindgen --target web`；`.wasm` 作为静态 `WebAssembly.Module` 打包。 |
| Netlify、Supabase、EdgeOne、Appwrite Deno | `wasm-bindgen --target deno`；wasm base64 内联，运行时 instantiate。 |
| Deno Deploy | `main.ts` 加生成的 `pkg/` 目录。 |

Cloudflare 不允许从 byte buffer 做任意 runtime wasm compilation，因此走 static module
路径。Deno-family 目标可从 bytes instantiate，使用自包含 bundle 可以避免平台打包时丢失
sibling `.wasm` 文件。

## 部署检查清单

1. 创建 Turso 数据库和 token。
2. 决定使用 Upstash，还是使用 libSQL KV fallback 作为 cache。
3. 如果使用 sealed secrets，生成并保存 `GPROXY_MASTER_KEY`。
4. 上传平台 bundle。
5. 配置 secrets，包括 admin 用户名和密码。
6. 把 gateway、admin、user 和 ops 路径都路由到 worker/function。
7. 需要 Web UI 且平台 bundle 未内置站点根路径 Console 时，同源提供 Console 静态资产。

## 平台说明

Cloudflare Workers 使用 `deploy/cloudflare/wrangler.toml`、compiled wasm rule，以及
服务 `/console` 的 Worker static assets binding。一键部署流程会要求填写必需的 Turso
和 admin secrets。CLI 部署时，先用 `wrangler secret put` 设置 `TURSO_URL`、
`TURSO_TOKEN`、`GPROXY_ADMIN_USER` 和 `GPROXY_ADMIN_PASSWORD`，再在
`deploy/cloudflare` 中运行 `wrangler deploy`。

Netlify 使用 `deploy/netlify/netlify.toml` 和 `edge-functions/` 入口。用
`netlify env:set` 设置环境变量，再执行 `netlify deploy --prod`。一键部署流程通过
`[template.environment]` 要求填写必需的 Turso 环境变量和 admin 凭据。publish 目录会包含
Console SPA；`/console/*` 会从 edge function 中排除，由 Netlify 静态文件和 SPA fallback
服务。

Supabase 使用 `deploy/supabase/functions/gproxy`，部署命令应包含
`supabase functions deploy gproxy --no-verify-jwt`。当 API upload path 会丢 sibling
wasm 文件时，不要使用该路径。

EdgeOne Pages 使用 `deploy/eopages/gproxy`，需要较新的 `edgeone` CLI。生成的 catch-all
edge function 接收 API 路径，`/` 和 `/console/*` 由平台静态内容加一个小的 SPA fallback
edge function 服务。

Deno Deploy 使用包含 `main.ts`、`pkg/` 和 `deno.json` 的 compact root。当前路径使用新的
Deno Deploy CLI module，不走旧 Deploy Classic `deployctl`。upload root 也包含 Console
构建产物，`main.ts` 会先服务 `/console/*`，再把 API 流量交给 wasm。

Appwrite Functions 通过 `deno-2.0` runtime 运行预构建 wasm。不要用 Appwrite 的 Rust
runtime 部署这个 bundle。Appwrite 默认函数 URL 不是站点根路径，所以 release bundle 不声明
内置 Console。

## Edge 限制

edge runtime 尽量共享相同 routing engine、transform pipeline、admin/user dispatcher
和 protocol logic，但少数 native-only API 返回 `501 not_implemented`：

- `/admin/update/*`
- `/admin/login-flows/cookie`
- `/admin/credentials/{id}/usage`

ops endpoints（`/healthz`、`/version`、`/metrics`）在 edge 上和 native 一样需要 admin 鉴权。
