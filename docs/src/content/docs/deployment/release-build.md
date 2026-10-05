---
title: "Building from Source"
description: "Build every GPROXY v4 target from source — the server, the desktop shell, the Worker and the console — and run the quality gates CI runs."
---

The Release workflow publishes `dev` pushes to `nightly` (the dev update channel),
and `main` pushes to `staging` (the beta update channel). Stable version releases
come from `main` and update both release and beta; they never overwrite dev.
Versioned releases are built on
`v*` tags matching the workspace version. The instructions below cover source
builds; `.github/workflows/release.yml` drives automated packaging.

Before pushing dev, run `bash scripts/push-dev.sh` to rebase on the latest main.
Enable the local push guard with `git config core.hooksPath .githooks`.
Public attachments contain packages, `manifest.json`, and `SHA256SUMS`;
individual checksum files and build provenance remain internal CI inputs.

## Release packages

CLI (`gproxy-*`) and Application (`gproxy-tauri-*`) are separate programs;
each has its own portable archives and installers. Applications embed the Console.

| Platform | CLI | Application |
| --- | --- | --- |
| Linux GNU (x86_64, aarch64, riscv64) | ZIP, DEB | ZIP, DEB |
| Linux musl (x86_64, aarch64, riscv64) | ZIP, DEB, Alpine APK | ZIP, DEB, Alpine APK |
| Windows (x86_64, aarch64) | ZIP, MSIX | ZIP, MSIX |
| macOS (x86_64, aarch64) | ZIP, DMG | ZIP, DMG |
| Android (x86_64, aarch64) | ZIP, Termux DEB | APK only |
| OpenHarmony / HarmonyOS NEXT | ARM64 / x86_64 ZIP | ARM64 unsigned HAP |

The Linux CLI DEB installs `gproxy` under `/usr/bin`; the Termux DEB installs
under `/data/data/com.termux/files/usr`. Android CLI builds use the source
recipe in `distribution/termux/` and a pinned official Termux builder.
Both DEB and ZIP use Termux's `libc++`, OpenSSL and CA certificates; ZIPs include
the launcher, executable and `TERMUX.txt` with installation/update instructions.
Self-update is disabled in these builds: install a newer DEB with APT, or use
`pkg upgrade gproxy` once an enabled repository provides it. Windows CLI
MSIX uses a distinct `.CLI` identity and a console execution alias. macOS CLI
DMGs include the executable and Terminal installation instructions.
Application ZIPs retain desktop resources and, on macOS, the complete `.app`.

OHOS builds use a cached toolchain image and a pinned experimental Tauri branch;
other platforms retain stable Tauri. HAP files require signing before installation.
The HAP has been verified on an emulator. Continuous background execution is implemented, but availability depends on the device, OS version and granted permissions; the proxy stops when the application process exits.

Nightly filenames stay fixed; the manifest records the commit SHA. GNU Linux
Applications require the distribution's GTK 3 and WebKitGTK 4.1 packages. Alpine
APKs target musl; musl Application DEBs bundle their graphical runtime and use
Bubblewrap. macOS uses ad-hoc signing; Developer ID signing and
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

## Build environment

Install stable Rust (edition 2024), Go, Clang, Node.js 22.12+ (24 LTS recommended), and pnpm. Linux desktop builds also need the `webkit2gtk-4.1`, `gtk+-3.0`, and `libsoup-3.0` development packages. The release workflow configures the other platform toolchains.

## Build the CLI and console

From the repository root:

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build
cargo build -p gproxy --release
./target/release/gproxy serve --data-dir ./data
```

The console build copies resources into the HTTP and Tauri embed directories. A subsequent Rust build includes them. A fresh checkout built with Rust alone has no console assets, so `/console` returns 404. You can also pass `--console-path console/dist` to serve a separate build.

CLI defaults include all channels, memory cache, local file storage, and bundled vocabulary. Select fewer channels or add backends as needed:

```sh
cargo build -p gproxy --release --no-default-features \
  --features embedded-console,memory,fs,codex,claudecode,openai,custom
```

PostgreSQL, MySQL, Redis, and S3 require `postgres`, `mysql`, `redis`, and `s3` respectively. SQLite is always available.

### Headless CLI for CI

The Release workflow builds headless ZIP packages independently of the frontend:

| Platform | Architectures | Package |
| --- | --- | --- |
| Linux GNU | x86_64, aarch64, riscv64 | `gproxy-headless-linux-<arch>.zip` |
| Linux musl | x86_64, aarch64, riscv64 | `gproxy-headless-linux-<arch>-musl.zip`, `gproxy-headless-linux-<arch>-musl.apk` |
| Windows | x86_64, aarch64 | `gproxy-headless-windows-<arch>.zip` |
| macOS | x86_64, aarch64 | `gproxy-headless-macos-<arch>.zip` |
| Android (Termux) | x86_64, aarch64 | `gproxy-headless-android-<arch>.zip` |

Download from [Releases](https://github.com/LeenHawk/gproxy/releases): `nightly`
for dev, `staging` for beta, or a stable version. Self-update selects the matching
headless package. On Android, run `pkg install libc++ openssl ca-certificates`,
extract the ZIP under Termux's home directory, then run `./gproxy serve --console=false`.
On Windows, use `gproxy.exe` in place of `gproxy`.

This variant excludes the bundled Web console and retains proxy routes,
management APIs, all channels, SQLite, memory cache, local file storage and
bundled vocabulary. Configure it through the CLI or management API. It needs no
Node.js, pnpm or desktop libraries to build:

```sh
cargo build --locked --release -p gproxy --bin gproxy \
  --no-default-features --features channels,memory,fs,bundled-vocabulary
./target/release/gproxy serve --console=false
```

Omitting `embedded-console` excludes frontend assets even when the checkout
already contains a built console. `--console=false` also disables serving an
external console directory. To build a console-enabled variant with a custom
feature set, add `embedded-console`.

## Build Application

After building the console:

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

A new instance opens the setup wizard. The window manages it over IPC, while the HTTP listener serves gateway clients. Use the release workflow's platform toolchains and packaging scripts for mobile builds.

The default workspace members exclude Tauri so backend builds do not require desktop dependencies. `--workspace` includes it.

## Build Workers

From the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

`worker-build` compiles and optimizes WASM. `check` runs a Wrangler deployment dry run without publishing. See [Edge deployment](/deployment/edge/) for configuration and runtime restrictions.

## Validate changes

Backend default members:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Check the desktop host separately:

```sh
cargo clippy -p gproxy-host-tauri --all-targets -- -D warnings
cargo test -p gproxy-host-tauri
```

WASM checks include supported libraries and hosts, not the CLI or Tauri. Do not use the entire workspace for this target:

```sh
cargo check --target wasm32-unknown-unknown \
  -p gproxy-protocol -p gproxy-store -p gproxy-seaorm -p gproxy-client \
  -p gproxy-cache -p gproxy-channel -p gproxy-core -p gproxy-file \
  -p gproxy-tokenizer -p gproxy-sdk -p gproxy-app -p gproxy-host-axum \
  -p gproxy-host-edge
```

Console and docs:

```sh
pnpm --dir console lint
pnpm --dir console test
pnpm --dir console build
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs check
pnpm --dir docs build
bash scripts/check-docs.sh
```

After changing Rust DTOs, run `pnpm --dir console types` to regenerate the TypeScript types.

## Publish

Run `bash scripts/release.sh` from a clean checkout included in `main`. Before
pushing a stable tag, it updates Console and docs npm dependencies with
`pnpm update --latest`, upgrades Rust registry requirements with `cargo upgrade`
(including major and pinned versions), and refreshes `Cargo.lock` with `cargo update`.
Compatibility bridges for RSA/getrandom, JNI/Tauri, Codex reqwest and WebSocket
configs retain their upstream-required version lines; compatible updates still
refresh through `cargo update`.
Install the Rust helper with `cargo install cargo-edit --locked` first.
If these commands change tracked files, the script stops before creating or
pushing the tag. Resolve compatibility changes, run the checks above, commit and
push to `main`, then rerun the script. Prerelease tags skip this dependency refresh.

A version tag must match the workspace version and have a `docs/release-notes/v<version>.md` file. The workflow builds packages, creates a signed update manifest, and uploads assets. A build or signing failure means publication is incomplete.

Version tags also invoke `scripts/publish-crates.sh` for selected MIT libraries. Other crates use git or path dependencies; see [Embedding the core](/reference/embedding/).
