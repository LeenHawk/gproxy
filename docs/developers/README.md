# Developer Guide

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
