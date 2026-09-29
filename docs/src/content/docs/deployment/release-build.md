---
title: "Building from Source"
description: "Build every GPROXY v4 target from source — the server, the desktop shell, the Worker and the console — and run the quality gates CI runs."
---

The Release workflow builds nightly on pushes to `dev`, and versioned releases on
`v*` tags matching the workspace version. The instructions below cover source
builds; `.github/workflows/release.yml` drives automated packaging.

## Release packages

CLI (`gproxy-*`) and Application (`gproxy-tauri-*`) are separate programs;
each has its own portable archives and installers. Applications embed the Console.

| Platform | CLI | Application |
| --- | --- | --- |
| Linux GNU (x86_64, aarch64, riscv64) | ZIP, DEB | ZIP, DEB |
| Linux musl (x86_64, aarch64, riscv64) | ZIP, DEB | — |
| Windows (x86_64, aarch64) | ZIP, MSIX | ZIP, MSIX |
| macOS (x86_64, aarch64) | ZIP, DMG | ZIP, DMG |
| Android (x86_64, aarch64) | ZIP, Termux DEB | APK only |
| OpenHarmony / HarmonyOS NEXT | ARM64 / x86_64 ZIP | ARM64 experimental unsigned HAP |

The Linux CLI DEB installs `gproxy` under `/usr/bin`; the Termux DEB installs
under `/data/data/com.termux/files/usr` and carries its C++ runtime privately.
Android CLI ZIPs include the launcher, executable and C++ runtime. Windows CLI
MSIX uses a distinct `.CLI` identity and a console execution alias. macOS CLI
DMGs include the executable and Terminal installation instructions.
Application ZIPs retain desktop resources and, on macOS, the complete `.app`.

OHOS builds use a cached toolchain image and a pinned experimental Tauri branch;
other platforms retain stable Tauri. HAP files require signing before installation.
Device execution has not been verified; background services are not implemented.

Nightly filenames also carry a commit SHA prefix. Linux x86_64 builds on Ubuntu
22.04 and ARM64 on Ubuntu 24.04; installation requires the distribution's
WebKitGTK 4.1 packages. macOS uses ad-hoc signing; Developer ID signing and
notarization are not configured.

Windows uses the Microsoft Store package identity. Configure four variables in
the `release` environment: `MS_STORE_IDENTITY_NAME`, `MS_STORE_DISPLAY_NAME`,
`MS_STORE_IDENTITY_PUBLISHER`, and `MS_STORE_PUBLISHER_DISPLAY_NAME`. Stable
releases retain both architecture packages for Store submission. With
`MS_STORE_PUBLISH_ENABLED` enabled, successful GitHub publication triggers the
Store submission workflow. The MSIX files on GitHub are unsigned, just like the
Store submission packages; the Store signs them for distribution. They are not
trusted packages that can be installed by double-clicking. The app requires the
system WebView2 Runtime.

Android APKs are signed and verified using the existing `ANDROID_SIGNING_*`
secrets. Each app's `<target-triple>-tauri-apk` entry joins the Ed25519-signed
update manifest, separately from CLI ZIP updates. The legacy server-wrapper APK is no longer built. Missing required keys or Store identity variables fail packaging.

For a local build, prepare the Console and invoke the packaging script:

```sh
pnpm --dir console build
node console/scripts/sync-to-embed.mjs
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
TARGET_OS=linux TARGET_TRIPLE=x86_64-unknown-linux-gnu \
  ARTIFACT_NAME=gproxy-tauri-linux-x86_64 \
  GPROXY_BUILD_VERSION=$(scripts/release-metadata.sh version) \
  scripts/package-tauri-release.sh
```

Outputs go to `dist/release/`. The workflow also publishes native server ZIPs,
Termux packages, Edge bundles, and GNU/musl container images. Application
packages receive build provenance attestations hosted on GitHub.

## Prerequisites

| Tool | Needed for |
| --- | --- |
| A stable Rust toolchain (edition 2024) | every crate |
| `wasm32-unknown-unknown` | the Workers host, and the CI check |
| `webkit2gtk-4.1`, `gtk+-3.0`, `libsoup-3.0` (Linux) | the desktop shell |
| Node.js LTS and pnpm | the console and this documentation site |
| `worker-build` | the Workers bundle |

## The Workspace

Sixteen crates. `cargo check`, `cargo test` and `cargo clippy` with no `-p` and
no `--workspace` build the **default members**, which is everything except the
desktop shell.

That exclusion is not a demotion and it is not about incremental builds: with a
warm target directory the two selections are within half a second of each
other. It is about the **cold** one. The desktop shell brings wry, webkit, gtk
and their `-sys` crates — about a hundred and eighty extra third-party crates
that have nothing to say about the engine, and that a Linux box without the
development headers cannot build at all.

`--workspace` builds it, and CI does.

## The Server

```sh
cargo build -p gproxy --release
./target/release/gproxy --version
```

```text
gproxy 4.0.0-dev
```

The default features are `channels`, `memory`, `fs` and `bundled-vocabulary` —
a single-node SQLite instance with every channel compiled in. Name channels one
by one for a binary that carries only the upstreams a deployment uses:

```sh
cargo build -p gproxy --release --no-default-features \
  --features memory,fs,codex,claudecode,openai,custom
```

Add `postgres`, `mysql`, `redis` or `s3` as the deployment needs. SQLite is
always compiled in, and a backend this build does not have is refused at
startup naming the feature that would provide it.

## The Console

```sh
cd console
pnpm install --frozen-lockfile
pnpm build
```

A release build copies `console/dist` into
`crates/gproxy-host-axum/assets/web` **before** `cargo build`, and the bundle
is embedded with `rust-embed`.

**A source checkout embeds nothing**, and that is the intended state:
`cargo build` produces a binary whose console paths answer 404 rather than a
blank page that looks like a broken application, and the startup log says so.

For development against a Vite build, point the binary at a directory instead:

```sh
GPROXY_CONSOLE_PATH=console/dist ./target/release/gproxy serve
```

The TypeScript types the console is written against are **generated from
Rust**, never written by hand, from two crates into two directories:

```sh
GPROXY_TS_OUT=console/src/generated/sdk cargo test -p gproxy-sdk --features ts export_types
GPROXY_TS_OUT=console/src/generated/app cargo test -p gproxy-app --features ts export_types
```

Without the variable each test returns immediately and writes nothing, so
`cargo test --all-features` stays hermetic and a generated directory is only
ever rewritten on purpose. The two go to **separate** directories because the
export wipes its output first, and two crates sharing one would erase each
other.

## The Desktop Shell

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

One process, one instance, two front doors: the window over Tauri IPC for the
management and user surfaces, and a real axum host on `127.0.0.1:8787` serving
the **data plane only**, for the CLIs that speak HTTP and cannot speak IPC.

The test suite drives the whole arrangement on a machine with **no display
server**, because almost everything is in the library and the binary only opens
a window.

```sh
cargo check  -p gproxy-host-tauri
cargo clippy -p gproxy-host-tauri --all-targets --all-features -- -D warnings
cargo test   -p gproxy-host-tauri
```

The first-run wizard supports launch-at-login and a system tray. Desktop auto-update is not implemented.

## The Worker

```sh
cargo install worker-build
CARGO_PROFILE_RELEASE_STRIP=none worker-build --release -- --no-default-features --features d1,custom,codex,claudecode
```

See [Edge (Cloudflare Workers)](/deployment/edge/) for the bindings, the
configuration document and the size constraint that makes naming channels one
by one worth doing.

## The Quality Gates

Exactly what CI runs:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
```

The last one is not optional decoration. `gproxy-app` and the axum router both
build for wasm, which is what lets the Workers host mount the same router
instead of writing the route table a second time — and a handler that forgets
its `Send` bridge is **a compile error on that target naming the handler**. The
check is the enforcement.

A lint finding gets a code change, not an `#[allow]`.

Per-crate, while working on one:

```sh
cargo test   -p gproxy-channel --all-features
cargo clippy -p gproxy-channel --all-features --target wasm32-unknown-unknown --lib -- -D warnings
cargo test   -p gproxy-host-axum
cargo test   -p gproxy-store -p gproxy-seaorm
```

Every `gproxy-host-axum` integration test builds the **real router** over an
in-memory instance, with only the upstream scripted. A test that called a
handler function directly would skip the part being tested.

Two suites bind a real loopback port because they cannot be faked: a websocket
round trip, because the in-process service harness never produces hyper's
upgrade extension; and a client disconnect, because every HTTP client in the
tree drains a body before handing it over, so that suite types the request out
over a raw socket and hangs up by dropping it.

## This Documentation Site

```sh
cd docs
pnpm install --frozen-lockfile
pnpm check
pnpm build
```

Astro Starlight, deployed to Cloudflare Pages by CI. `pnpm check` validates the
notification feed the site also hosts.

`scripts/check-docs.sh` is the structural check: sidebar slugs against pages,
English and Chinese parity, frontmatter, forbidden references and oversized
pages.

## Library releases

Version tags also invoke `scripts/publish-crates.sh` for the selected MIT library
crates. Other workspace crates are consumed through git or path dependencies;
see [Embedding the Core](/reference/embedding/). The public surface is not stable.
