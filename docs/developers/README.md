# gproxy — Developer Guide

gproxy v2 is a **single Rust crate** (lib + bin) that compiles to three targets
from one codebase:

- a **native** server (axum + tokio),
- a **wasm** edge worker (`fetch` handler, same router), and
- the same crate wrapped as an **Appwrite** Rust function.

The React **console** is a separate `pnpm` app, built and embedded into the
native binary via `rust-embed`.

---

## Prerequisites

- **Rust** stable (edition 2024) + `rustfmt`, `clippy`
- **Node 22** + **pnpm 9** (console)
- For edge builds: `wasm32-unknown-unknown` target + `wasm-bindgen-cli`
  **pinned to the `wasm-bindgen` version in `Cargo.lock`** (currently 0.2.123),
  and `wasm-opt` (binaryen) for the size-optimized eopages/supabase bundles.

---

## Build

```bash
# 1. Console (emits to console/dist and syncs into assets/console for embedding)
cd console && pnpm install --frozen-lockfile && pnpm build && cd ..

# 2. Native binary (default = lean production feature set)
cargo build --release                       # -> target/release/gproxy

# 3. Edge wasm (lib only, edge features, no native deps)
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
bash deploy/<platform>/build.sh             # generate the platform glue bundle
```

`cargo build --features full` adds every native backend (redis, all SQL
databases) for development.

### Feature flags

| Feature | Effect |
|---|---|
| `default` | `cache-memory` + `persist-db` + `persist-file` + `upstream-wreq` + `count-local` + `migrate-v1` |
| `full` | default + `cache-redis` |
| `edge` | `cache-libsql` + `cache-upstash` + `persist-libsql` + `upstream-fetch` (wasm target) |
| `migrate-v1` | one-shot v1→v2 SQLite migration on boot (`src/app/migrate_v1/`) |

The native build **requires** `upstream-wreq`; edge builds use `upstream-fetch`.

---

## Layout

```text
src/
  main.rs              # CLI + config + state wiring + serve
  lib.rs               # public modules (also the wasm + appwrite entry surface)
  http/
    server/            # native axum router (the canonical handlers)
    edge/              # wasm fetch entry — dispatches to the same handlers
    client/            # UpstreamClient: WreqClient (native) / FetchClient (edge)
  app/                 # AppState, control-plane snapshot, bootstrap, migrate_v1
  store/               # persistence + cache backends
  channel/ process/ transform/ protocol/   # routing + protocol engine
console/               # React console (TanStack Router, Tailwind, shadcn)
deploy/                # per-platform edge bundles + build scripts (see below)
docs/                  # architecture + deployment docs
```

---

## Test & lint

```bash
cargo fmt --all --check
cargo clippy --features full --all-targets -- -D warnings
cargo test  --features full
cargo check --lib --no-default-features --features edge --target wasm32-unknown-unknown
cd console && pnpm typecheck && pnpm test && pnpm i18n:check
```

Conventions (see `CLAUDE.md`): no TDD / no over-testing — test only tricky logic
and real-bug regressions; one file ≤ 200 lines ideal, 500 hard cap; run
`cargo fmt` + `cargo clippy` after every change; commits carry no AI co-author
line.

---

## CI / release

- **`.github/workflows/ci.yml`** — console (typecheck/test/build/i18n) + backend
  (fmt/clippy/test/edge-wasm-check) on push to `main` and PRs.
- **`.github/workflows/release.yml`** (on a published Release) — builds **native
  binaries** (linux gnu/musl, android, windows, macOS × x86_64/aarch64; UPX;
  zip+sha256), **edge wasm bundles** (all six platforms), and **Docker** images
  (gnu/musl × amd64/arm64 → GHCR + manifest).

---

## Edge targets

Each `deploy/<platform>/` holds the hand-written entry + config; the
`wasm-bindgen` glue (`_lib/`) is generated build output (gitignored).
`deploy/<platform>/build.sh` regenerates it. The two capability tiers:

- **Static `?module` import** (Cloudflare, Vercel) — `wasm-bindgen --target web`.
- **Runtime instantiate / base64 inline** (Deno, Netlify, Supabase, EdgeOne) —
  `wasm-bindgen --target deno`.

Deploy commands + per-platform gotchas: **[../edge-deploy.md](../edge-deploy.md)**.

## Appwrite (deno-2.0, via wasm)

`deploy/appwrite-deno/main.ts` runs gproxy on Appwrite Functions as a **Deno**
function that serves the pre-built edge wasm (Appwrite never compiles Rust). It
bridges `context.req` → the wasm `fetch` export → `context.res`. The native
`rust-1.83` runtime can't build gproxy (Cargo 1.83 vs edition 2024, `handler`
crate name, default features, ~10 min build cap) — the wasm path sidesteps all of
it. See [deploy/appwrite-deno/NOTES.md](../../deploy/appwrite-deno/NOTES.md).

## License

[AGPL-3.0-or-later](../../LICENSE).
