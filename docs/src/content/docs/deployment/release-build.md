---
title: "Building & Releases"
description: "Build gproxy and the edge wasm from source, run the quality gates, and follow the tag-driven pipeline that signs and publishes every artifact"
---

GPROXY ships as one native binary, `gproxy`, built from
`crates/gproxy-host-axum`, plus a wasm host, `crates/gproxy-host-edge`, for
fetch-based platforms. The operator console is compiled once and embedded
into the native binary, so a source build always starts with the console.

## Prerequisites

| Tool | Used for |
| --- | --- |
| Rust stable toolchain (edition 2024) | Every crate; add the `wasm32-unknown-unknown` target for the edge host |
| Node.js LTS and pnpm 9 | Console (`console/`) and docs (`docs/`) |
| `wasm-bindgen-cli` matching `Cargo.lock`, or `wasm-pack` | Edge glue generation |
| Docker with buildx | Container image |
| `cross`, `cargo-ndk`, WiX 4, `dpkg-deb`, `hdiutil` | Release packaging only |

## Build the Console

```sh
cd console
pnpm install --frozen-lockfile
pnpm build
```

`pnpm build` runs `tsc -b`, `vite build`, and `scripts/sync-to-embed.mjs`,
which copies `console/dist/` into `crates/gproxy-host-axum/assets/web/`. That
directory is gitignored apart from `.gitkeep`; the native host embeds it with
`rust-embed` at compile time. If you skip this step the binary still serves
the API, but `/`, `/admin`, and `/portal` answer `404` with the text
`web assets are not embedded; run pnpm build in console/ and rebuild gproxy`.

## Build the Native Binary

```sh
cargo build --release -p gproxy-host-axum
./target/release/gproxy --version
```

The binary is `target/release/gproxy`. `cargo run -p gproxy-host-axum` runs
a debug build with the defaults (`127.0.0.1:8787`, `./data`, SQLite).
`--version` prints the build identity:

```text
gproxy 3.0.0-alpha.0 (channel development, build 4054fe4f94ea, installation source)
```

The identity is fixed at compile time from these variables, read with
`option_env!` in `crates/gproxy-host-axum/src/lib.rs`:

| Variable | Default | Release value |
| --- | --- | --- |
| `GPROXY_BUILD_VERSION` | Cargo package version | Workspace version |
| `GPROXY_BUILD_CHANNEL` | `development` | `releases` or `dev` |
| `GPROXY_BUILD_HASH` | Short git hash from `build.rs`, else `unknown` | Commit SHA |
| `GPROXY_INSTALLATION_KIND` | `source` | `standalone`, `container`, or `android-apk` |
| `GPROXY_UPDATE_PUBKEY` | Unset | Base64 Ed25519 public key, 32 bytes |

Without `GPROXY_UPDATE_PUBKEY` the binary has no key to verify update
manifests or the announcement feed against, so both verifications fail. A
development build does not need it.

## Build the Edge Wasm

```sh
rustup target add wasm32-unknown-unknown
cargo build -p gproxy-host-edge --release --target wasm32-unknown-unknown
wasm-bindgen --target bundler --out-dir deploy/cloudflare/pkg \
  --out-name gproxy_host_edge \
  target/wasm32-unknown-unknown/release/gproxy_host_edge.wasm
```

Cloudflare uses the `bundler` target; Deno and Netlify use `--target web`.
The `wasm-bindgen` CLI version must equal the `wasm-bindgen` crate version in
`Cargo.lock`. `scripts/package-edge-release.sh` performs the build, generates
both glue variants, copies a prebuilt `console/dist` into each
`deploy/<platform>/public/`, and zips the three bundles. The platform
directories also carry `pnpm run build` / `deno task build` scripts that do
the same through `wasm-pack`; see [Edge Wasm](/deployment/edge/).

## Quality Gates

Backend and console changes finish with the same commands CI runs:

| Command | Checks |
| --- | --- |
| `cargo fmt --check` | Formatting |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lints; any warning fails |
| `cargo test --workspace` | Tests; also regenerates the TypeScript DTOs |
| `cargo check --workspace --target wasm32-unknown-unknown` | The core still compiles for edge |
| `pnpm lint` (in `console/`) | `tsc -b`, ESLint, locale parity (`pnpm i18n:check`) |
| `pnpm test` (in `console/`) | Vitest and the model-catalog script tests |
| `pnpm build` (in `console/`) | Production bundle |

The admin API DTOs derive `ts_rs::TS`; `cargo test` writes them to
`console/src/generated/`. Those files are generated output: change the Rust
type, run `cargo test`, and commit the result. Never edit them by hand.

## CI

`.github/workflows/ci.yml` runs on every push and pull request with three
jobs: **Backend** (the four cargo gates above), **Console**
(`pnpm install --frozen-lockfile`, lint, test, build, `i18n:check`), and
**Docs** (`pnpm check`, `pnpm build` in `docs/`). Pushes to the default
branch or `3.0` also run **Deploy docs**, which signs `notifications.json`
with the update signing key (producing `notifications.json.sig`) and
publishes the site to Cloudflare Pages. The native binary polls that feed
and verifies it with the same compiled-in public key.

## Cutting a Release

```sh
scripts/release.sh
```

The script reads `[workspace.package].version` from `Cargo.toml`, requires a
clean tracked worktree, creates the annotated tag `v<version>` if it does not
exist, refuses if the tag points at another commit, and pushes only that tag.
Re-running it for the same commit is safe. A version with a prerelease suffix
(`3.0.0-alpha.0`) builds the `dev` channel; a plain version builds
`releases`.

## Release Pipeline

The tag push runs `.github/workflows/release.yml`. Jobs, in order:

1. **Release metadata** — verifies the tag equals `v<workspace version>`,
   derives the channel, and loads the target matrix from
   `scripts/release-targets.json`.
2. **Console and container image** — builds the console once through the
   Dockerfile's `console-dist` stage, builds and pushes
   `ghcr.io/leenhawk/gproxy:<tag>` (linux/amd64, with BuildKit provenance
   and SBOM attestations), and saves the same image as
   `gproxy-container-linux-amd64.tar.gz`. The console output is passed to
   the later jobs as a workflow artifact.
3. **Native `<target>`** — one job per matrix row. Each downloads the
   console into `crates/gproxy-host-axum/assets/web`, checks that the update
   public key decodes to 32 bytes, builds `--bin gproxy` with `cargo`,
   `cross`, or `cargo-ndk` (Android API 28), and packages the result.
   Windows builds set `RUSTFLAGS=-C target-feature=+crt-static`; macOS
   binaries are ad hoc signed with `codesign --sign -`.
4. **Signed update manifest** — `scripts/build-update-manifest.sh` collects
   every native zip and Android APK, records `target_triple`, `url`,
   `sha256`, and `size` for each, derives `min_compatible_data_version` from
   the `Control` schema version in `crates/gproxy-store/src/schema/catalog.rs`,
   and signs the canonical payload with the Ed25519 private key. It aborts
   when the private and public keys do not match.
5. **Edge bundles** — installs the matching `wasm-bindgen-cli`, runs
   `scripts/package-edge-release.sh`, and type-checks the three platform
   entries (`pnpm check`, `deno check`).
6. **Publish release** — creates or updates the GitHub release `v<version>`
   (`--prerelease` for `dev` builds) and uploads every file. For `dev`
   builds it also force-moves the `dev` tag to the commit and uploads
   `manifest.json` to the fixed prerelease named `dev`, unless a newer v3
   prerelease already exists.

### Native Targets

| Artifact | Target triple | Builder | Installer |
| --- | --- | --- | --- |
| `gproxy-linux-x86_64` | `x86_64-unknown-linux-gnu` | cargo | `.deb` |
| `gproxy-linux-aarch64` | `aarch64-unknown-linux-gnu` | cargo (arm runner) | `.deb` |
| `gproxy-linux-riscv64` | `riscv64gc-unknown-linux-gnu` | cross | `.deb` |
| `gproxy-linux-x86_64-musl` | `x86_64-unknown-linux-musl` | cross | `.deb` |
| `gproxy-linux-aarch64-musl` | `aarch64-unknown-linux-musl` | cross | `.deb` |
| `gproxy-linux-riscv64-musl` | `riscv64gc-unknown-linux-musl` | cross | `.deb` |
| `gproxy-macos-x86_64` | `x86_64-apple-darwin` | cargo | `.dmg` |
| `gproxy-macos-aarch64` | `aarch64-apple-darwin` | cargo | `.dmg` |
| `gproxy-windows-x86_64` | `x86_64-pc-windows-msvc` | cargo | `.msi` |
| `gproxy-windows-aarch64` | `aarch64-pc-windows-msvc` | cargo | `.msi` |
| `gproxy-android-x86_64` | `x86_64-linux-android` | cargo-ndk | `.apk` |
| `gproxy-android-aarch64` | `aarch64-linux-android` | cargo-ndk | `.apk` |

Every artifact has a `.zip` (binary, `README.md`, `LICENSE`), the installer
listed, a `.sha256` beside each file, and a `.provenance.json`. Android zips
contain the ELF as `gproxy.bin`, the NDK `libc++_shared.so`, and a `gproxy`
launcher script; the APK wraps the same payload. The release also carries
`manifest.json`, `gproxy-edge.wasm`,
`gproxy-edge-{cloudflare,deno,netlify}.zip`, and
`gproxy-container-linux-amd64.tar.gz`, each with a checksum. Exact names live
in `scripts/release-targets.json` and the workflow.

## Signing

| Mechanism | Signs | CI secrets |
| --- | --- | --- |
| macOS ad hoc `codesign --sign -` | The binary and the `.app` inside the DMG | none |
| Android `apksigner` | Every `.apk` | `ANDROID_SIGNING_KEYSTORE_B64`, `ANDROID_SIGNING_KEYSTORE_PASSWORD`, `ANDROID_SIGNING_KEY_ALIAS`, optional `ANDROID_SIGNING_KEY_PASSWORD` |
| Ed25519 update key | `manifest.json` and `notifications.json` | `UPDATE_SIGNING_PRIVATE_KEY_B64` (base64 PEM), `UPDATE_SIGNING_PUBLIC_KEY_B64` (base64 raw key) |

The public half is compiled into every binary as `GPROXY_UPDATE_PUBKEY`, so a
binary accepts only manifests and announcements signed by the matching
private key. Generate a pair in the form the scripts expect:

```sh
openssl genpkey -algorithm ed25519 -out update.pem
base64 -w0 update.pem                                    # UPDATE_SIGNING_PRIVATE_KEY_B64
openssl pkey -in update.pem -pubout -outform DER \
  | tail -c 32 | base64 -w0                              # UPDATE_SIGNING_PUBLIC_KEY_B64
```

The docs deploy additionally needs `CLOUDFLARE_API_TOKEN`,
`CLOUDFLARE_ACCOUNT_ID`, and `CLOUDFLARE_PROJECT_ID`.

## Build Provenance

`scripts/build-provenance.sh` writes one `<artifact>.provenance.json` per
artifact: `version`, `commit`, `tag`, `target`, `builder`, the `rustc`,
`node`, and `pnpm` versions that ran, and, for every `FROM` line in the
Dockerfile, the image reference and the digest it resolved to at build time.
Image tags float inside a pinned line; this record is what identifies a
build later.

## Update Channels

A released binary checks for updates against a signed manifest:

| Channel | Manifest URL |
| --- | --- |
| `releases` | `https://github.com/LeenHawk/gproxy/releases/latest/download/manifest.json` |
| `staging` | `https://github.com/LeenHawk/gproxy/releases/download/staging/manifest.json` |
| `dev` | `https://github.com/LeenHawk/gproxy/releases/download/dev/manifest.json` |

The compiled channel is the default; the console's update channel setting or
`GPROXY_UPDATE_CHANNEL` overrides it, and `GPROXY_UPDATE_SERVE` points at a
self-hosted manifest. GitHub's `releases/latest` never resolves to a
prerelease, so alpha builds are compiled with channel `dev` and follow the
`dev` manifest. See [Configuration](/reference/configuration/) for the
remaining native-only variables.
