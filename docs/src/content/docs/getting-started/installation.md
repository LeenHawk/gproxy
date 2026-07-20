---
title: Installation
description: Install GPROXY v2 from a release binary, Docker image, source build, or edge bundle.
---

GPROXY v2 is a single Rust crate that builds one native binary named `gproxy`.
The same crate also builds the edge WebAssembly runtime. The React console is
not a separate service in the native build: after `console/` is built, its
static files are synced into `assets/console/` and embedded in the binary.

Choose the installation path that matches how you want to run it.

:::tip[Start with a prebuilt download]
Most users should choose a package on the [Downloads](/getting-started/downloads/)
page or the [latest GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest).
Do not build from source unless you are developing GPROXY or need a custom build.
:::

## Installable Packages

Release builds include native installers as well as portable ZIP archives:

| Platform | Package | Background behavior |
| --- | --- | --- |
| Android | `.apk` | A foreground service keeps GPROXY alive with a persistent notification. The launcher controls app-launch and device-boot startup and requests background permission when it is enabled. |
| Windows | `.msi` | Installs per-user under Local AppData and adds a Start menu entry. First-run setup asks whether to start hidden at login. |
| macOS | `.dmg` | Drag `GPROXY.app` to Applications. First-run setup can register a per-user LaunchAgent; the app runs without a Dock icon. |
| Linux | `.deb` | Installs a desktop launcher and an XDG login entry. First-run setup asks whether to enable it. Packages are published for amd64, arm64, and RISC-V 64. |

The MSI, DMG, and DEB launchers ask for an administrator username, a non-blank
password, and the login-startup preference on first run. Setup can also create
enabled providers for any selected built-in channels and generate an API
key for the administrator. The key is copied or displayed once; save it before
closing the dialog. Setup lists every channel included in the native build;
providers start without credentials, and `custom` also needs its base URL, so
finish their settings, API-key, or interactive login setup in Console. Launchers
save the username but
not the plaintext password or generated API key; GPROXY persists the Argon2id
password hash and the API-key digest/secret for later authentication. Afterward,
open **Settings → Background Service** in the
embedded Console to change login startup. Turning it off does not stop the
currently running server. Startup entries never copy admin passwords, master
keys, DSNs, or authenticated proxy URLs. Deployments that depend on those
values should use their existing service manager instead.

Background launcher output is written to `%LOCALAPPDATA%\GPROXY\logs\gproxy.log`
and `gproxy-error.log` on Windows,
`~/Library/Logs/GPROXY/gproxy.log` on macOS, and
`${XDG_STATE_HOME:-~/.local/state}/gproxy/gproxy.log` on Linux.

## Portable Release Binary

Use a release binary when you want the native server with the embedded console
and no local Rust or Node toolchain.

1. Download the archive for your OS and CPU from the
   [Downloads](/getting-started/downloads/) page or the
   [latest GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest).
2. Extract the archive. On Android, keep `gproxy`, `gproxy.bin`, and
   `libc++_shared.so` in the same directory.
3. Put it somewhere on your `PATH` or run it directly.

```bash
chmod +x ./gproxy
./gproxy --help
```

Release archives are built by the v2 release workflow for Linux, macOS, Windows,
and Android. Linux builds cover x86_64, AArch64, and RISC-V 64, with GNU and
musl variants. The same three Linux architectures are published in both the
default GNU Docker image and the `-musl` image. Android releases also include
per-ABI APKs for users who prefer an installable package over the raw archive.

The Android APK requires Android 9 (API 28) or newer and includes a launcher UI
and foreground service. Install the matching ABI APK and open **GPROXY**. Enter
an administrator username and password, optionally choose any built-in channels
and administrator API-key generation, then tap **Start GPROXY**. The generated
key is copied and displayed once. The
launcher switch controls later app-launch and device-boot startup. When
the switch is enabled, the app explains why background operation is needed and
opens Android's battery-optimization permission prompt. The service runs the
native server with:

```text
GPROXY_ADMIN_USER=<username>
--host 127.0.0.1 --port 8787 --data-dir <app-private-data>/data
```

The launcher does not store plaintext passwords. GPROXY persists the Argon2id
hash for login verification. After initial setup, leaving the password field
blank starts the existing administrator unchanged; entering a password
intentionally resets it. Android displays a persistent notification while the
service is running.

Release APKs use the package name `io.github.leenhawk.gproxy`, app label
`GPROXY`, and the Console favicon as the launcher icon. Published release APKs
must be signed with the Android signing secrets configured in GitHub Actions.

Then open:

```text
http://127.0.0.1:8787/console
```

## Docker Image

The published image is `ghcr.io/leenhawk/gproxy`.

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker run --rm -p 8787:8787 \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

The Docker image already contains a prebuilt native binary with the embedded
console. The image defaults to `GPROXY_HOST=0.0.0.0`,
`GPROXY_PORT=8787`, `GPROXY_PERSISTENCE=db`, and `GPROXY_DATA_DIR=/app/data`.

See [Docker](/deployment/docker/) for persistent volumes, PostgreSQL/MySQL DSNs,
and tag selection.

## Build From Source (Developers Only)

:::caution
This section is for GPROXY development and custom builds. If you only want to
run GPROXY, use the [Downloads](/getting-started/downloads/) page instead; do not
clone the repository or install Rust and Node just to get the application.
:::

Use a source build only when you are developing GPROXY or need a local build
before a release exists.

Prerequisites:

- A current stable Rust toolchain with edition 2024 support.
- Node.js and pnpm if you want the embedded console to match current
  `console/` sources. The release workflow uses Node 22 and pnpm 9.
- Platform libraries required by your Rust target.

Build the console first when its assets should be embedded:

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..
```

Then build the binary from the repository root:

```bash
cargo build --release --bin gproxy
./target/release/gproxy --help
```

`pnpm build` creates `console/dist/` and then runs
`console/scripts/sync-to-embed.mjs`, which syncs the built files to
`assets/console/`. `rust-embed` compiles that directory into the native binary.

If you skip the console build, the gateway and admin APIs can still compile and
run, but `/console` may return `console assets not embedded`.

## Edge Bundles

Do not ask edge platforms to compile the Rust source. The supported edge path is
to upload a prebuilt bundle:

```text
build wasm in CI or on a machine with Rust -> generate platform bundle -> upload bundle
```

Release artifacts include platform zip files such as
`gproxy-edge-cloudflare.zip`, `gproxy-edge-netlify.zip`, and
`gproxy-edge-deno.zip`.

See [Edge Wasm Deployment](/deployment/edge/) for platform-specific commands and
runtime secrets.

## Next Steps

- Use [Downloads](/getting-started/downloads/) to select another platform or
  package format.
- Continue with [Quick Start](/getting-started/quick-start/) to boot a local
  instance.
- Read [Embedded Console](/guides/console/) before putting the native server
  behind a reverse proxy.
- Read [Migrating From v1 To v2](/deployment/v1-to-v2/) before pointing v2 at an
  existing v1 data directory.
