# Edge wasm 部署

gproxy v2 的 edge 版本是同一个 Rust crate 编译出来的 `wasm32-unknown-unknown`
library。平台入口只负责三件事：加载 wasm-bindgen glue，调用 Rust 导出的
`init(...)` 建立 `AppState`，然后把每个请求交给 wasm `fetch`。

不要让边缘平台从源码仓库现编 Rust。多数平台没有 cargo，仓库里的 wasm-bindgen glue
目录又是 gitignored 构建产物。正确模型是：

```text
有 Rust 工具链的机器/CI 构建 wasm -> 生成平台 bundle -> 上传预构建 bundle
```

## 运行时依赖

edge 运行时没有本地 SQLite、PostgreSQL 或 MySQL 直连能力。控制面依赖 HTTP 数据服务：

| 变量 | 必需 | 用途 |
| --- | --- | --- |
| `TURSO_URL` | 是 | libSQL/Turso 控制面数据库。 |
| `TURSO_TOKEN` | 是 | Turso 访问令牌。 |
| `UPSTASH_URL` | 否 | Upstash Redis cache；缺省时回退到 libSQL KV。 |
| `UPSTASH_TOKEN` | 否 | Upstash 访问令牌。 |
| `GPROXY_MASTER_KEY` | 否 | base64 32 字节主密钥，用于解开已封装 secret。 |

这些是运行时 secret，应放在平台的 secret/env 系统里，不写进源码或 bundle。

## 获取预构建 bundle

推荐使用 GitHub Release 附带的 edge artifacts。发布流程会构建 wasm，并把平台入口、
配置、glue 和校验文件打包成 zip。下载后解压即可用平台 CLI 部署。

本地构建适合验证或临时发布：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge

bash deploy/<platform>/build.sh
```

`wasm-bindgen-cli` 必须匹配 `Cargo.lock` 里的 `wasm-bindgen` crate 版本。当前锁定版本为
`0.2.123`。

## 平台能力分组

不同平台接受 wasm 的方式不同，bundle 生成脚本也不同：

| 分组 | 平台 | bundle 形态 |
| --- | --- | --- |
| 静态 wasm module | Cloudflare Workers | `wasm-bindgen --target web`，平台把 `.wasm` 打成 `WebAssembly.Module`。 |
| inline/base64 runtime instantiate | Netlify、Supabase、EdgeOne Pages、Appwrite Deno | `wasm-bindgen --target deno`，把 wasm base64 内联进 bundle。 |
| Deno Deploy compact upload | Deno Deploy | `wasm-bindgen --target deno`，上传 `main.ts` + `pkg/`。 |

Cloudflare 不允许任意 buffer 的 runtime wasm compile，所以走静态 module。Netlify、
Supabase、EdgeOne、Appwrite Deno 可在运行时 `WebAssembly.instantiate(bytes, imports)`，
因此用自包含的 base64 bundle，避免 sibling `.wasm` 在平台打包时丢失。

## 通用本地构建

从仓库根目录执行：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
```

然后按平台生成 glue：

```bash
bash deploy/cloudflare/build.sh
bash deploy/netlify/build.sh
bash deploy/supabase/build.sh
bash deploy/eopages/build.sh
bash deploy/appwrite-deno/build.sh
```

`deploy/deno/build.sh` 是例外：它会自己执行 cargo build，并直接调用新的 Deno Deploy
CLI 发布到目标 app。

## Cloudflare Workers

生成 bundle：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/cloudflare/build.sh
```

设置 secret：

```bash
cd deploy/cloudflare
echo -n "$TURSO_URL" | wrangler secret put TURSO_URL
echo -n "$TURSO_TOKEN" | wrangler secret put TURSO_TOKEN
echo -n "$UPSTASH_URL" | wrangler secret put UPSTASH_URL
echo -n "$UPSTASH_TOKEN" | wrangler secret put UPSTASH_TOKEN
echo -n "$GPROXY_MASTER_KEY" | wrangler secret put GPROXY_MASTER_KEY
```

部署：

```bash
wrangler deploy
```

`wrangler.toml` 通过 `CompiledWasm` rule 把 `_lib/gproxy_bg.wasm` 打包成
`WebAssembly.Module`。入口文件 `src/worker.js` 在首次请求时懒初始化 Rust `AppState`。

## Netlify Edge Functions

生成自包含 bundle：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/netlify/build.sh
```

设置站点环境变量：

```bash
cd deploy/netlify
netlify env:set TURSO_URL "$TURSO_URL"
netlify env:set TURSO_TOKEN "$TURSO_TOKEN"
netlify env:set UPSTASH_URL "$UPSTASH_URL"
netlify env:set UPSTASH_TOKEN "$UPSTASH_TOKEN"
netlify env:set GPROXY_MASTER_KEY "$GPROXY_MASTER_KEY"
```

部署：

```bash
netlify deploy --prod
```

`netlify.toml` 已把 edge functions 目录指向仓库使用的 `edge-functions/`。生成的
`_lib/` 目录只作为 import 模块存在，不是单独函数。

## Supabase Edge Functions

生成自包含 bundle：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/supabase/build.sh
```

设置 secret：

```bash
cd deploy/supabase
supabase secrets set \
  TURSO_URL="$TURSO_URL" \
  TURSO_TOKEN="$TURSO_TOKEN" \
  UPSTASH_URL="$UPSTASH_URL" \
  UPSTASH_TOKEN="$UPSTASH_TOKEN" \
  GPROXY_MASTER_KEY="$GPROXY_MASTER_KEY" \
  --project-ref "$SUPABASE_PROJECT_REF"
```

部署：

```bash
supabase functions deploy gproxy \
  --project-ref "$SUPABASE_PROJECT_REF" \
  --no-verify-jwt \
  --network-id host
```

不要使用 `--use-api`。Supabase 的 API 上传路径不会带上 sibling wasm；改成内联后又可能
超过请求体限制。默认 deploy 路径会用本地 Docker/eszip 打包，函数运行时仍然是
Supabase 托管的 Deno，不是你的容器。

如果本机用 Podman 代替 Docker：

```bash
systemctl --user start podman.socket
export DOCKER_HOST="unix:///run/user/$(id -u)/podman/podman.sock"
```

WSL2/netavark 环境下 `--network-id host` 可避开本地打包网络问题。

## EdgeOne Pages

生成 bundle：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/eopages/build.sh
```

部署：

```bash
edgeone pages deploy deploy/eopages/gproxy \
  --name <project-name> \
  -t "$EDGEONE_PAGES_API_TOKEN" \
  -e production
```

设置环境变量：

```bash
edgeone pages env set TURSO_URL "$TURSO_URL" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set TURSO_TOKEN "$TURSO_TOKEN" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set UPSTASH_URL "$UPSTASH_URL" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set UPSTASH_TOKEN "$UPSTASH_TOKEN" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set GPROXY_MASTER_KEY "$GPROXY_MASTER_KEY" -t "$EDGEONE_PAGES_API_TOKEN"
```

EdgeOne Pages 需要 `edgeone` CLI `>= 1.5.9`，旧版本的 root catch-all 路由有已验证 bug。
当前 bundle 使用 `edge-functions/[[default]].js` 接住所有路径，`/` 静态首页仍由平台精确匹配。

预览域名 `*.edgeone.run` 会带 `eo_token` / `eo_time` 保护。用 curl 验证时需要携带部署输出里的
query，并跟随重定向保存 cookie。

## Deno Deploy

`deploy/deno/build.sh` 是构建并部署脚本：

```bash
set -a
source ./.env
set +a
bash deploy/deno/build.sh
```

需要：

```bash
DENO_DEPLOY_TOKEN=...
DENO_DEPLOY_PROJECT=gproxy-deno     # 可选，默认 gproxy-deno
DENO_DEPLOY_ORG=leenhawk20          # 可选，默认 leenhawk20
```

脚本会创建 compact upload root：`main.ts` + `pkg/` + `deno.json`，然后使用新的
Deno Deploy CLI 模块：

```bash
deno run -A https://jsr.io/@deno/deploy/0.0.99/main.ts --prod <upload-root>
```

不要走旧的 Deploy Classic `deployctl` 路径；新项目创建已经被 Deno 官方阻断。

## Appwrite Functions

Appwrite 使用 `deno-2.0` runtime 跑预构建 wasm，不使用 Appwrite 的 Rust runtime。

生成 bundle：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/appwrite-deno/build.sh
```

配置 CLI：

```bash
appwrite client --endpoint https://<region>.cloud.appwrite.io/v1 \
  --project-id <PROJECT_ID> \
  --key <API_KEY>
```

创建并部署函数：

```bash
appwrite functions create \
  --function-id gproxy-wasm \
  --name gproxy-wasm \
  --runtime deno-2.0 \
  --execute any

appwrite push functions --function-id gproxy-wasm --activate
```

设置环境变量：

```bash
appwrite functions create-variable --function-id gproxy-wasm \
  --variable-id TURSO_URL --key TURSO_URL --value "$TURSO_URL"
```

`TURSO_TOKEN` 必填；`UPSTASH_URL`、`UPSTASH_TOKEN`、`GPROXY_MASTER_KEY` 可选。

Appwrite 的 `rust-1.83` runtime 不能作为 v2 路径：Cargo 版本不满足 edition 2024，
平台期望 crate 名为 `handler`，并且默认 feature/构建时限也不适合这个项目。

## 验证

edge ops 端点和 native 一样需要 admin 鉴权。匿名访问 `/healthz`、`/version`、`/metrics`
应该返回 401，而不是公开健康信息。

```bash
curl -i "$EDGE_URL/healthz"

curl -i "$EDGE_URL/healthz" \
  -H "Authorization: Bearer <admin-user-api-key>"
```

网关路径也应进入 pipeline 并要求 user API key：

```bash
curl -i "$EDGE_URL/v1/models"
curl -i "$EDGE_URL/openai/v1/models"
```

预期是未带 key 时返回 gproxy 的 JSON 401，而不是平台静态 404 或函数初始化错误。

## 常见问题

| 问题 | 处理 |
| --- | --- |
| 平台自动从 Git 构建失败 | 不要让平台现编 Rust；部署 release artifact 或本地生成的 bundle。 |
| `missing required env var: TURSO_URL` | 平台运行时 secret 没配置，或 Netlify/EdgeOne 初始化发生在 env 注入前；确认使用当前懒初始化入口。 |
| Supabase 500 / 找不到 wasm | 确认使用 `deploy/supabase/build.sh` 生成内联 bundle，并且部署时没有 `--use-api`。 |
| EdgeOne 预览 401 `eo_time missing` | 使用部署输出里的 `eo_token` / `eo_time` query，并保存重定向后的 cookie。 |
| `/healthz` 匿名 401 | 正常；ops 端点是 admin-gated。 |
| console 页面能打开但 API 失败 | 确认 console 静态资源与 edge API 同源，或按 `docs/deployment.md` 配置同源反代。 |

## 相关页面

- `docs/deployment.md`：console 静态资源部署形态。
- `deploy/README.md`：部署目录清单。
- `deploy/<platform>/NOTES.md`：平台实测记录和约束。

## English

# Edge Wasm Deployment

The edge build of gproxy v2 is the same Rust crate compiled as a
`wasm32-unknown-unknown` library. Platform entry code does only three things:
load wasm-bindgen glue, call the Rust-exported `init(...)` to build `AppState`,
and forward each request to the wasm `fetch`.

Do not ask edge platforms to compile Rust from the source repository. Most of
them do not have cargo, and the wasm-bindgen glue directories in this repository
are gitignored build output. The correct model is:

```text
build wasm on a machine/CI with Rust -> generate platform bundle -> upload prebuilt bundle
```

## Runtime Dependencies

Edge runtimes cannot connect to local SQLite, PostgreSQL, or MySQL directly. The
control plane uses HTTP data services:

| Variable | Required | Purpose |
| --- | --- | --- |
| `TURSO_URL` | Yes | libSQL/Turso control-plane database. |
| `TURSO_TOKEN` | Yes | Turso access token. |
| `UPSTASH_URL` | No | Upstash Redis cache; falls back to libSQL KV when absent. |
| `UPSTASH_TOKEN` | No | Upstash access token. |
| `GPROXY_MASTER_KEY` | No | Base64 32-byte master key for opening sealed secrets. |

These are runtime secrets. Store them in the platform secret/env system, not in
source files or bundles.

## Getting A Prebuilt Bundle

Prefer GitHub Release edge artifacts. The release workflow builds wasm and packs
platform entries, config, glue, and checksum files into zip artifacts. Download
and unzip them, then deploy with the platform CLI.

Local builds are useful for verification or temporary releases:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge

bash deploy/<platform>/build.sh
```

`wasm-bindgen-cli` must match the `wasm-bindgen` crate version in `Cargo.lock`.
The current locked version is `0.2.123`.

## Platform Capability Groups

Platforms accept wasm in different ways, so bundle scripts differ:

| Group | Platforms | Bundle shape |
| --- | --- | --- |
| Static wasm module | Cloudflare Workers | `wasm-bindgen --target web`; platform packages `.wasm` as a `WebAssembly.Module`. |
| Inline/base64 runtime instantiate | Netlify, Supabase, EdgeOne Pages, Appwrite Deno | `wasm-bindgen --target deno`; wasm is base64-inlined into the bundle. |
| Deno Deploy compact upload | Deno Deploy | `wasm-bindgen --target deno`; upload `main.ts` plus `pkg/`. |

Cloudflare does not allow arbitrary runtime wasm compilation from buffers, so it
uses a static module. Netlify, Supabase, EdgeOne, and Appwrite Deno can call
`WebAssembly.instantiate(bytes, imports)` at runtime, so they use self-contained
base64 bundles to avoid losing sibling `.wasm` files during platform packaging.

## Common Local Build

From the repository root:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
```

Then generate platform glue:

```bash
bash deploy/cloudflare/build.sh
bash deploy/netlify/build.sh
bash deploy/supabase/build.sh
bash deploy/eopages/build.sh
bash deploy/appwrite-deno/build.sh
```

`deploy/deno/build.sh` is the exception: it runs cargo itself and then deploys
through the new Deno Deploy CLI module.

## Cloudflare Workers

Generate the bundle:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/cloudflare/build.sh
```

Set secrets:

```bash
cd deploy/cloudflare
echo -n "$TURSO_URL" | wrangler secret put TURSO_URL
echo -n "$TURSO_TOKEN" | wrangler secret put TURSO_TOKEN
echo -n "$UPSTASH_URL" | wrangler secret put UPSTASH_URL
echo -n "$UPSTASH_TOKEN" | wrangler secret put UPSTASH_TOKEN
echo -n "$GPROXY_MASTER_KEY" | wrangler secret put GPROXY_MASTER_KEY
```

Deploy:

```bash
wrangler deploy
```

`wrangler.toml` uses a `CompiledWasm` rule to package `_lib/gproxy_bg.wasm` as a
`WebAssembly.Module`. `src/worker.js` lazily initializes Rust `AppState` on the
first request.

## Netlify Edge Functions

Generate the self-contained bundle:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/netlify/build.sh
```

Set site environment variables:

```bash
cd deploy/netlify
netlify env:set TURSO_URL "$TURSO_URL"
netlify env:set TURSO_TOKEN "$TURSO_TOKEN"
netlify env:set UPSTASH_URL "$UPSTASH_URL"
netlify env:set UPSTASH_TOKEN "$UPSTASH_TOKEN"
netlify env:set GPROXY_MASTER_KEY "$GPROXY_MASTER_KEY"
```

Deploy:

```bash
netlify deploy --prod
```

`netlify.toml` points edge functions at the repository's `edge-functions/`
directory. The generated `_lib/` directory is an imported module, not a separate
function.

## Supabase Edge Functions

Generate the self-contained bundle:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/supabase/build.sh
```

Set secrets:

```bash
cd deploy/supabase
supabase secrets set \
  TURSO_URL="$TURSO_URL" \
  TURSO_TOKEN="$TURSO_TOKEN" \
  UPSTASH_URL="$UPSTASH_URL" \
  UPSTASH_TOKEN="$UPSTASH_TOKEN" \
  GPROXY_MASTER_KEY="$GPROXY_MASTER_KEY" \
  --project-ref "$SUPABASE_PROJECT_REF"
```

Deploy:

```bash
supabase functions deploy gproxy \
  --project-ref "$SUPABASE_PROJECT_REF" \
  --no-verify-jwt \
  --network-id host
```

Do not use `--use-api`. Supabase's API upload path does not include sibling
wasm files; making the wasm inline can then exceed request body limits. The
default deploy path uses local Docker/eszip packaging, while the function still
runs in Supabase-hosted Deno, not inside your container.

If Podman replaces Docker locally:

```bash
systemctl --user start podman.socket
export DOCKER_HOST="unix:///run/user/$(id -u)/podman/podman.sock"
```

In WSL2/netavark environments, `--network-id host` can avoid local packaging
network issues.

## EdgeOne Pages

Generate the bundle:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/eopages/build.sh
```

Deploy:

```bash
edgeone pages deploy deploy/eopages/gproxy \
  --name <project-name> \
  -t "$EDGEONE_PAGES_API_TOKEN" \
  -e production
```

Set environment variables:

```bash
edgeone pages env set TURSO_URL "$TURSO_URL" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set TURSO_TOKEN "$TURSO_TOKEN" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set UPSTASH_URL "$UPSTASH_URL" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set UPSTASH_TOKEN "$UPSTASH_TOKEN" -t "$EDGEONE_PAGES_API_TOKEN"
edgeone pages env set GPROXY_MASTER_KEY "$GPROXY_MASTER_KEY" -t "$EDGEONE_PAGES_API_TOKEN"
```

EdgeOne Pages needs `edgeone` CLI `>= 1.5.9`; older root catch-all routing has a
verified bug. The current bundle uses `edge-functions/[[default]].js` to catch
all paths, while `/` static index still uses the platform's exact match.

Preview domains under `*.edgeone.run` include `eo_token` / `eo_time` protection.
When verifying with curl, include the query from deployment output and follow
redirects while saving cookies.

## Deno Deploy

`deploy/deno/build.sh` is a build-and-deploy script:

```bash
set -a
source ./.env
set +a
bash deploy/deno/build.sh
```

It needs:

```bash
DENO_DEPLOY_TOKEN=...
DENO_DEPLOY_PROJECT=gproxy-deno     # optional, default gproxy-deno
DENO_DEPLOY_ORG=leenhawk20          # optional, default leenhawk20
```

The script creates a compact upload root with `main.ts`, `pkg/`, and
`deno.json`, then uses the new Deno Deploy CLI module:

```bash
deno run -A https://jsr.io/@deno/deploy/0.0.99/main.ts --prod <upload-root>
```

Do not use the old Deploy Classic `deployctl` path; Deno has blocked creation of
new projects there.

## Appwrite Functions

Appwrite runs prebuilt wasm through the `deno-2.0` runtime. It does not use
Appwrite's Rust runtime.

Generate the bundle:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/appwrite-deno/build.sh
```

Configure the CLI:

```bash
appwrite client --endpoint https://<region>.cloud.appwrite.io/v1 \
  --project-id <PROJECT_ID> \
  --key <API_KEY>
```

Create and deploy the function:

```bash
appwrite functions create \
  --function-id gproxy-wasm \
  --name gproxy-wasm \
  --runtime deno-2.0 \
  --execute any

appwrite push functions --function-id gproxy-wasm --activate
```

Set environment variables:

```bash
appwrite functions create-variable --function-id gproxy-wasm \
  --variable-id TURSO_URL --key TURSO_URL --value "$TURSO_URL"
```

`TURSO_TOKEN` is required; `UPSTASH_URL`, `UPSTASH_TOKEN`, and
`GPROXY_MASTER_KEY` are optional.

Appwrite `rust-1.83` is not a supported v2 path: the Cargo version does not
support edition 2024, the platform expects the crate to be named `handler`, and
the default feature set/build time are not suitable for this project.

## Verification

Edge ops endpoints require admin auth just like native. Anonymous access to
`/healthz`, `/version`, and `/metrics` should return 401, not public health
data.

```bash
curl -i "$EDGE_URL/healthz"

curl -i "$EDGE_URL/healthz" \
  -H "Authorization: Bearer <admin-user-api-key>"
```

Gateway paths should enter the pipeline and require a user API key:

```bash
curl -i "$EDGE_URL/v1/models"
curl -i "$EDGE_URL/openai/v1/models"
```

Without a key, the expected result is gproxy's JSON 401, not a platform static
404 or function initialization error.

## Troubleshooting

| Problem | Fix |
| --- | --- |
| Platform build-from-Git fails | Do not let the platform compile Rust; deploy a release artifact or local bundle. |
| `missing required env var: TURSO_URL` | Runtime secret is missing, or Netlify/EdgeOne initialized before env injection; confirm the current lazy init entry is used. |
| Supabase 500 / wasm not found | Use `deploy/supabase/build.sh` to generate the inline bundle and deploy without `--use-api`. |
| EdgeOne preview 401 `eo_time missing` | Use the `eo_token` / `eo_time` query from deployment output and keep redirected cookies. |
| Anonymous `/healthz` returns 401 | Normal; ops endpoints are admin-gated. |
| Console loads but API fails | Ensure console static assets and edge API are same-origin, or configure same-origin reverse proxy as in `docs/deployment.md`. |

## Related Pages

- `docs/deployment.md`: console static asset deployment shapes.
- `deploy/README.md`: deployment directory inventory.
- `deploy/<platform>/NOTES.md`: platform-specific field notes and constraints.
