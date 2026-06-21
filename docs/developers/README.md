# Developer Guide

## English

This page is the day-to-day entry point for working on gproxy v2. Unlike v1,
the v2 rewrite is not a multi-crate Cargo workspace: it is one Rust crate with a
native binary, a wasm/edge library surface, and a separate React console that is
embedded into the native build.

Use the root `README.md` for product-facing setup, `docs/architecture-design.md`
for the system model, and this page for the commands and boundaries that matter
while changing the repository.

## Repository Shape

```text
.
|-- Cargo.toml              # single crate: lib + bin, edition 2024
|-- src/
|   |-- main.rs             # native CLI, config loading, AppState, server boot
|   |-- lib.rs              # shared module surface and wasm/app entry points
|   |-- http/
|   |   |-- server/         # native Axum router and console asset host
|   |   |-- edge/           # wasm fetch adapter using the same handler layer
|   |   |-- admin_api/      # admin/user API dispatcher shared by native + edge
|   |   `-- client/         # upstream transports: wreq native, fetch edge
|   |-- app/                # bootstrap, snapshots, export, v1 migration
|   |-- store/              # persistence and cache backends
|   |-- channel/            # upstream providers, credentials, health, OAuth
|   |-- process/            # request routing, compile steps, execution surface
|   |-- transform/          # protocol transforms by operation
|   |-- protocol/           # OpenAI, Claude, Gemini wire types
|   |-- pipeline/           # failover, balance, settle, usage paths
|   `-- billing/ tokenize/ usage/ crypto/ ...
|-- console/                # React 19 console, Vite, TanStack Router, Tailwind
|-- assets/console/         # generated embed target for rust-embed
|-- deploy/                 # platform entries and build scripts for edge wasm
|-- upstream_docs/          # provider protocol/reference material
`-- docs/                   # architecture, deployment, and implementation docs
```

The native server and edge worker share the same application state and request
handler concepts. Platform-specific code should stay at the boundary: native
setup in `src/http/server/`, wasm request adaptation in `src/http/edge/`, and
provider CLIs or generated glue under `deploy/<platform>/`.

## Toolchain

- Rust stable with edition 2024 support, plus `rustfmt` and `clippy`.
- Node 22 and pnpm 9 for the console.
- `wasm32-unknown-unknown` for edge builds.
- `wasm-bindgen-cli` pinned to the version in `Cargo.lock` when generating edge
  bundles. The current lockfile uses `wasm-bindgen` 0.2.123.
- `wasm-opt` is useful for local size experiments, but CI intentionally avoids
  running it in the canonical edge build because optimized wasm can break the
  bindgen descriptor interpreter for this bundle.

## First Local Build

Build the console first when you want the native binary to serve the real SPA at
`/console`:

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..

cargo build --release
```

`pnpm build` runs TypeScript, Vite, and `scripts/sync-to-embed.mjs`. The sync
step copies `console/dist/` into `assets/console/`, which is the path compiled
into the native binary by `rust-embed`.

If the console has not been built, native `/console` still compiles, but it only
has the placeholder embed directory.

## Development Commands

Backend:

```bash
cargo fmt --all --check
cargo clippy --features full --all-targets -- -D warnings
cargo test --features full
cargo run --features full
```

Console:

```bash
cd console
pnpm typecheck
pnpm test
pnpm i18n:check
pnpm dev
```

For local browser work, run the backend separately with insecure cookies enabled
when using plain HTTP:

```bash
GPROXY_INSECURE_COOKIES=1 cargo run --features full
```

The Vite dev server proxies `/admin`, `/user`, `/healthz`, `/version`, and
`/metrics` to `http://127.0.0.1:8787`, so the console can use same-origin
cookies during development.

## Feature Sets

The default native build is intentionally smaller than the full development
build:

| Feature | Purpose |
| --- | --- |
| `default` | `cache-memory`, `persist-db`, `persist-file`, `upstream-wreq`, `count-local`, `migrate-v1` |
| `full` | Default native features plus `cache-redis`; use this for broad local checks. |
| `edge` | Wasm-only edge features: `cache-libsql`, `cache-upstash`, `persist-libsql`, `upstream-fetch`. |
| `migrate-v1` | One-shot startup migration from a v1 SQLite database into the v2 schema. |

Native upstream calls require `upstream-wreq`. Edge builds must use
`upstream-fetch` and must be compiled without default native features:

```bash
cargo check --lib --no-default-features --features edge --target wasm32-unknown-unknown
```

## Edge Builds

The edge worker is built from the same crate as a wasm library:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/<platform>/build.sh
```

Each `deploy/<platform>/build.sh` regenerates the platform's wasm-bindgen glue
from `target/wasm32-unknown-unknown/release/gproxy.wasm`. Generated glue
directories are build output and are ignored by git.

The release workflow packages ready-to-deploy bundles for Cloudflare,
EdgeOne Pages, Netlify, Supabase, Appwrite Deno, and Deno Deploy. Edge platforms
should deploy those prebuilt bundles or locally generated equivalents; do not
point platform "build from Git" flows at the source checkout and expect cargo to
exist there.

See `docs/edge-deploy.md` for platform-specific deploy notes.

## Appwrite

`deploy/appwrite-deno/` runs gproxy as an Appwrite Deno function by serving the
prebuilt wasm module and forwarding Appwrite's request/response objects through
the wasm `fetch` export. Appwrite's Rust runtime is not the supported path for
v2 because it cannot build this edition 2024 crate within the platform limits.

See `deploy/appwrite-deno/NOTES.md` for the exact runtime notes.

## CI and Release

CI runs on pushes to `main` and pull requests:

- Console: install, typecheck, unit tests, i18n parity, build.
- Backend: format check, clippy with `full`, tests with `full`, and an edge wasm
  check with `--no-default-features --features edge`.

The release workflow is triggered by `workflow_dispatch` or a published GitHub
Release. It builds:

- native binaries for Linux GNU, Linux musl, Android, Windows, and macOS across
  supported x86_64/aarch64 targets;
- edge wasm bundles plus checksums;
- Docker images for GNU and musl runtime variants on amd64 and arm64;
- a refreshed orphan `deploy` branch containing prebuilt edge artifacts only.

## Change Discipline

Follow the project rules in `CLAUDE.md`:

- Do not use TDD for this project; add focused tests only for tricky logic or
  real regressions.
- Keep files small and split by responsibility.
- Prefer existing modules and patterns over new abstraction layers.
- Run `cargo fmt` and `cargo clippy` before finishing backend changes.
- Do not add AI co-author lines to commits.

For protocol work, keep the v2 design rule from the rewrite effort: organize
request behavior by operation and operation group, not by provider family. The
backend transform engine should remain permissive; provider-specific policy and
presets belong at the frontend/configuration boundary unless the runtime really
needs a new primitive.

## Related Pages

- `docs/architecture-design.md` - v2 architecture and request lifecycle.
- `docs/deployment.md` - console deployment shapes.
- `docs/edge-deploy.md` - edge wasm deployment model and platform notes.
- `docs/v1-to-v2-migration.md` - migration behavior from v1 data.
- `docs/generic-transform-rule-design-notes.md` - current transform rule design
  notes and unresolved schema questions.
- `deploy/README.md` - short inventory of deployment target directories.

## License

gproxy is licensed under AGPL-3.0-or-later.

## 中文

# 开发者指南

这页是日常开发 gproxy v2 的入口。和 v1 不同，v2 重写版不是多 crate 的 Cargo
workspace：它是一个 Rust crate，同时提供 native binary、wasm/edge library surface，
以及一个独立的 React console，console 构建后会嵌入 native binary。

产品级安装和使用看根目录 `README.md`；系统模型看 `docs/architecture-design.md`；
修改仓库时需要的命令和边界看本页。

## 仓库结构

```text
.
|-- Cargo.toml              # single crate: lib + bin, edition 2024
|-- src/
|   |-- main.rs             # native CLI, config loading, AppState, server boot
|   |-- lib.rs              # shared module surface and wasm/app entry points
|   |-- http/
|   |   |-- server/         # native Axum router and console asset host
|   |   |-- edge/           # wasm fetch adapter using the same handler layer
|   |   |-- admin_api/      # admin/user API dispatcher shared by native + edge
|   |   `-- client/         # upstream transports: wreq native, fetch edge
|   |-- app/                # bootstrap, snapshots, export, v1 migration
|   |-- store/              # persistence and cache backends
|   |-- channel/            # upstream providers, credentials, health, OAuth
|   |-- process/            # request routing, compile steps, execution surface
|   |-- transform/          # protocol transforms by operation
|   |-- protocol/           # OpenAI, Claude, Gemini wire types
|   |-- pipeline/           # failover, balance, settle, usage paths
|   `-- billing/ tokenize/ usage/ crypto/ ...
|-- console/                # React 19 console, Vite, TanStack Router, Tailwind
|-- assets/console/         # generated embed target for rust-embed
|-- deploy/                 # platform entries and build scripts for edge wasm
|-- upstream_docs/          # provider protocol/reference material
`-- docs/                   # architecture, deployment, and implementation docs
```

native server 和 edge worker 共享同一套应用状态与请求处理概念。平台相关代码应留在边界：
native setup 在 `src/http/server/`，wasm 请求适配在 `src/http/edge/`，平台 CLI 或生成胶水
在 `deploy/<platform>/`。

## 工具链

- 支持 edition 2024 的 Rust stable，以及 `rustfmt`、`clippy`。
- console 使用 Node 22 和 pnpm 9。
- edge 构建需要 `wasm32-unknown-unknown` target。
- 生成 edge bundle 时，`wasm-bindgen-cli` 要和 `Cargo.lock` 中的 `wasm-bindgen`
  crate 版本一致。当前 lockfile 使用 `wasm-bindgen` 0.2.123。
- `wasm-opt` 可用于本地体积实验，但 CI 的标准 edge build 不运行它，因为优化后的 wasm
  可能破坏这个 bundle 的 bindgen descriptor interpreter。

## 首次本地构建

如果希望 native binary 在 `/console` 服务真实 SPA，先构建 console：

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..

cargo build --release
```

`pnpm build` 会运行 TypeScript、Vite 和 `scripts/sync-to-embed.mjs`。同步步骤把
`console/dist/` 复制到 `assets/console/`，该目录由 `rust-embed` 编进 native binary。

如果还没有构建 console，native `/console` 仍然能编译，但只包含占位 embed 目录。

## 开发命令

后端：

```bash
cargo fmt --all --check
cargo clippy --features full --all-targets -- -D warnings
cargo test --features full
cargo run --features full
```

Console：

```bash
cd console
pnpm typecheck
pnpm test
pnpm i18n:check
pnpm dev
```

本地浏览器开发时，如果使用 plain HTTP，后端通常要开启 insecure cookies：

```bash
GPROXY_INSECURE_COOKIES=1 cargo run --features full
```

Vite dev server 会把 `/admin`、`/user`、`/healthz`、`/version`、`/metrics`
代理到 `http://127.0.0.1:8787`，所以开发态 console 可以使用 same-origin cookie。

## Feature Sets

默认 native build 比完整开发 build 更小：

| Feature | 作用 |
| --- | --- |
| `default` | `cache-memory`、`persist-db`、`persist-file`、`upstream-wreq`、`count-local`、`migrate-v1` |
| `full` | 默认 native feature 加上 `cache-redis`；适合宽范围本地检查。 |
| `edge` | wasm-only edge feature：`cache-libsql`、`cache-upstash`、`persist-libsql`、`upstream-fetch`。 |
| `migrate-v1` | 从 v1 SQLite 数据库一次性启动迁移到 v2 schema。 |

native 上游调用需要 `upstream-wreq`。edge build 必须使用 `upstream-fetch`，并且不能启用
默认 native feature：

```bash
cargo check --lib --no-default-features --features edge --target wasm32-unknown-unknown
```

## Edge 构建

edge worker 来自同一个 crate 的 wasm library：

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/<platform>/build.sh
```

每个 `deploy/<platform>/build.sh` 会从
`target/wasm32-unknown-unknown/release/gproxy.wasm` 重新生成平台对应的 wasm-bindgen glue。
生成目录是构建产物，已经被 git ignore。

release workflow 会为 Cloudflare、EdgeOne Pages、Netlify、Supabase、Appwrite Deno 和
Deno Deploy 打包可部署 bundle。edge 平台应该部署这些预构建 bundle，或部署本地生成的等价
bundle；不要让平台从源码 checkout 里现编 cargo。

平台细节看 `docs/edge-deploy.md`。

## Appwrite

`deploy/appwrite-deno/` 通过 Appwrite Deno function 运行 gproxy：它加载预构建 wasm
module，并把 Appwrite 的 request/response 对象转发给 wasm `fetch` export。Appwrite 的
Rust runtime 不是 v2 支持路径，因为平台无法在限制内构建 edition 2024 crate。

具体 runtime 说明见 `deploy/appwrite-deno/NOTES.md`。

## CI 与发布

CI 在 push 到 `main` 和 pull request 时运行：

- Console：install、typecheck、unit tests、i18n parity、build。
- Backend：format check、`full` feature clippy、`full` feature tests，以及
  `--no-default-features --features edge` 的 edge wasm check。

release workflow 由 `workflow_dispatch` 或 GitHub Release published 触发。它会构建：

- Linux GNU、Linux musl、Android、Windows、macOS 的 native binary，覆盖支持的
  x86_64/aarch64 target；
- edge wasm bundle 和 checksum；
- GNU/musl runtime variant 的 amd64/arm64 Docker image；
- 刷新的 orphan `deploy` branch，只包含预构建 edge artifacts。

## 修改纪律

遵守 `CLAUDE.md` 里的项目规则：

- 这个项目不走 TDD-heavy 流程；只为复杂逻辑或真实回归加聚焦测试。
- 文件保持小而按职责拆分。
- 优先复用现有模块和模式，不随意加新抽象层。
- 后端改动完成前运行 `cargo fmt` 和 `cargo clippy`。
- commit 不添加 AI co-author 行。

协议相关改动继续遵守 v2 rewrite 的设计规则：请求行为按 operation 和 operation group
组织，而不是按 provider family 组织。后端 transform engine 应保持宽松；provider-specific
policy 和 preset 优先放在前端/配置边界，除非 runtime 真正需要新 primitive。

## 相关页面

- `docs/architecture-design.md`：v2 架构和请求生命周期。
- `docs/deployment.md`：console 部署形态。
- `docs/edge-deploy.md`：edge wasm 部署模型和平台注意事项。
- `docs/v1-to-v2-migration.md`：v1 数据迁移行为。
- `docs/generic-transform-rule-design-notes.md`：当前通用转换规则设计笔记和未决 schema 问题。
- `deploy/README.md`：部署目标目录清单。

## 许可证

gproxy 使用 AGPL-3.0-or-later 许可证。
