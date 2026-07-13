---
title: Installation
description: Install GPROXY v2 from a release binary, Docker image, source build, or edge bundle.
---

GPROXY v2 is a single Rust crate that builds one native binary named `gproxy`.
The same crate also builds the edge WebAssembly runtime. The React console is
not a separate service in the native build: after `console/` is built, its
static files are synced into `assets/console/` and embedded in the binary.

Choose the installation path that matches how you want to run it.

## Installable Packages

Release builds include native installers as well as portable ZIP archives:

| Platform | Package | Background behavior |
| --- | --- | --- |
| Android | `.apk` | A foreground service keeps GPROXY alive with a persistent notification. Automatic start is on by default for the first app launch and device boot, and can be changed in the launcher. |
| Windows | `.msi` | Installs per-user under Local AppData, adds a Start menu entry, and starts hidden at login. |
| macOS | `.dmg` | Drag `GPROXY.app` to Applications. The app runs without a Dock icon and registers a per-user LaunchAgent on first start. |
| Linux | `.deb` | Installs a desktop launcher and an XDG background login entry. Packages are published for amd64 and arm64. |

On Windows, macOS, and Linux, open **Settings → Background Service** in the
embedded Console to turn login startup on or off. Turning it off does not stop
the currently running server. Startup entries never copy admin passwords,
master keys, DSNs, or authenticated proxy URLs. Deployments that depend on
those values should use their existing service manager instead.

Background launcher output, including the one-time random first-boot password,
is written to `%LOCALAPPDATA%\GPROXY\logs\gproxy.log` on Windows,
`~/Library/Logs/GPROXY/gproxy.log` on macOS, and
`${XDG_STATE_HOME:-~/.local/state}/gproxy/gproxy.log` on Linux.

## Portable Release Binary

Use a release binary when you want the native server with the embedded console
and no local Rust or Node toolchain.

1. Download the archive for your OS and CPU from the GitHub release.
2. Extract the archive. On Android, keep `gproxy`, `gproxy.bin`, and
   `libc++_shared.so` in the same directory.
3. Put it somewhere on your `PATH` or run it directly.

```bash
chmod +x ./gproxy
./gproxy --help
```

Release archives are built by the v2 release workflow for Linux, macOS, Windows,
Android, x86_64, and aarch64 targets. Linux release binaries are also used as
the input to the Docker image. Android releases also include per-ABI APKs for
users who prefer an installable package over the raw executable archive.

The Android APK includes a launcher UI and foreground service. Install the
matching ABI APK and open **GPROXY**. It starts automatically on first launch;
the launcher switch controls future app-launch and device-boot startup. The
service runs the native server with:

```bash
--host 127.0.0.1 --port 8787 --data-dir <app-private-data>/data --admin-user <username>
```

To choose a password before the first run, turn automatic startup off, stop the
service, fill the password field, and start it again. Passwords are passed only
to that start and are never stored by the launcher. If the field is blank,
GPROXY uses its normal first-boot random admin password and prints it in the app
log. Android displays a persistent notification while the service is running.

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
`GPROXY_PORT=8787`, `GPROXY_PERSISTENCE=file`, and
`GPROXY_DATA_DIR=/app/data`.

See [Docker](/deployment/docker/) for persistent volumes, PostgreSQL/MySQL DSNs,
and tag selection.

## Build From Source

Use a source build when you are developing GPROXY or need a local build before a
release exists.

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
`gproxy-edge-cloudflare.zip`, `gproxy-edge-netlify.zip`,
`gproxy-edge-supabase.zip`, `gproxy-edge-deno.zip`,
`gproxy-edge-eopages.zip`, and `gproxy-edge-appwrite-deno.zip`.

See [Edge Wasm Deployment](/deployment/edge/) for platform-specific commands and
runtime secrets.

## Next Steps

- Continue with [Quick Start](/getting-started/quick-start/) to boot a local
  instance.
- Read [Embedded Console](/guides/console/) before putting the native server
  behind a reverse proxy.
- Read [Migrating From v1 To v2](/deployment/v1-to-v2/) before pointing v2 at an
  existing v1 data directory.
