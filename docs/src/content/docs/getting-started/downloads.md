---
title: Downloads
description: "Release assets for GPROXY: native installers, portable archives, the container image, edge bundles, checksums, provenance, and the signed update manifest."
---

Every release is published on the GitHub Releases page under a `v3.x.y` tag:

<https://github.com/LeenHawk/gproxy/releases>

:::note[Stable and prerelease links]
`releases/latest` and every `releases/latest/download/...` URL resolve to
the newest stable v3 release. Prereleases are excluded from that link: open
the releases list and pick a `v3` prerelease if you want one. The v2 line
remains available on its `v2.x.y` tags.
:::

Do not clone the repository or compile GPROXY to run it. The assets below
contain the optimized binary with the console embedded. Building from source
is covered in [Building & Releases](/deployment/release-build/).

## Asset Names

Native assets are named `gproxy-<os>-<arch>[-musl].<ext>`. One release carries
these targets:

| Asset stem | Target | Portable | Installer |
| --- | --- | --- | --- |
| `gproxy-linux-x86_64` | x86_64-unknown-linux-gnu | `.zip` | `.deb` |
| `gproxy-linux-aarch64` | aarch64-unknown-linux-gnu | `.zip` | `.deb` |
| `gproxy-linux-riscv64` | riscv64gc-unknown-linux-gnu | `.zip` | `.deb` |
| `gproxy-linux-x86_64-musl` | x86_64-unknown-linux-musl | `.zip` | `.deb` |
| `gproxy-linux-aarch64-musl` | aarch64-unknown-linux-musl | `.zip` | `.deb` |
| `gproxy-linux-riscv64-musl` | riscv64gc-unknown-linux-musl | `.zip` | `.deb` |
| `gproxy-macos-x86_64` | x86_64-apple-darwin | `.zip` | `.dmg` |
| `gproxy-macos-aarch64` | aarch64-apple-darwin | `.zip` | `.dmg` |
| `gproxy-windows-x86_64` | x86_64-pc-windows-msvc | `.zip` | `.msi` |
| `gproxy-windows-aarch64` | aarch64-pc-windows-msvc | `.zip` | `.msi` |
| `gproxy-android-x86_64` | x86_64-linux-android | `.zip` | `.apk` |
| `gproxy-android-aarch64` | aarch64-linux-android | `.zip` | `.apk` |

Linux GNU builds link glibc. The `-musl` builds are static. Windows builds
link the C runtime statically.

## Native Installers

| Package | Platform | What it does |
| --- | --- | --- |
| `.deb` | Debian and Ubuntu families | Installs `/usr/bin/gproxy`, a desktop launcher, and an XDG autostart entry. |
| `.dmg` | macOS 11 or later | A `GPROXY.app` bundle that runs the server in the background and opens the console. |
| `.msi` | Windows | Per-user install under `%LOCALAPPDATA%\Programs\GPROXY` with Start menu and Startup shortcuts. |
| `.apk` | Android 9 (API 28) or later | A signed app with a foreground service, a launcher screen, and in-app updates. |

Behaviour, data locations, and log paths for each installer are on the
[Installation](/getting-started/installation/) page.

## Portable Archives

Each `.zip` contains the executable (`gproxy`, or `gproxy.exe` on Windows),
`README.md`, and `LICENSE`. Android archives contain `gproxy.bin`, a `gproxy`
launcher script, and `libc++_shared.so`; keep the three files together.

Extract, then run:

```bash
chmod +x ./gproxy
./gproxy --help
```

## Container Image

```bash
docker pull ghcr.io/leenhawk/gproxy:<tag>
```

`<tag>` is the Git tag of the release. The v3 workflow pushes only that
versioned tag, for `linux/amd64` only; `latest` is not a v3 tag and there is
no `-musl` variant. The same image is attached to the release as
`gproxy-container-linux-amd64.tar.gz` for `docker load`. See
[Container](/deployment/docker/) for volumes and environment.

## Edge Bundles

| Asset | Contents |
| --- | --- |
| `gproxy-edge-cloudflare.zip` | Cloudflare Workers project: Worker entry, `wrangler.toml`, wasm package, console assets. |
| `gproxy-edge-deno.zip` | Deno Deploy project: `main.ts`, `deno.json`, wasm package, console assets. |
| `gproxy-edge-netlify.zip` | Netlify Edge project: edge function, `netlify.toml`, wasm package, console assets. |
| `gproxy-edge.wasm` | The raw `wasm32-unknown-unknown` build, for a custom host. |

Upload a bundle; do not ask the platform to compile Rust. See
[Edge Wasm](/deployment/edge/).

## Checksums and Provenance

Every asset has a `.sha256` sidecar in `sha256sum` format:

```bash
sha256sum -c gproxy-linux-x86_64.zip.sha256
```

Every artifact also has a `<stem>.provenance.json` that records the version,
commit, tag, target triple, builder (`cargo`, `cross`, `cargo-ndk`, `docker`,
or `wasm-bindgen`), toolchain versions, and the base-image digests the build
resolved.

## Signed Update Manifest

`manifest.json` is the Ed25519-signed manifest the built-in updater reads. It
lists the channel, version, release-notes URL, the minimum compatible data
version, and one entry per target with URL, SHA-256, and size. The public key
is embedded in the binary at build time, so a binary accepts only manifests
signed by its own release pipeline.

| Channel | Manifest location | Content |
| --- | --- | --- |
| `releases` | `releases/latest/download/manifest.json` | Stable tags without a prerelease suffix, `v3.0.0` and later. Stable builds default to this channel. |
| `staging` | `releases/download/staging/manifest.json` | Continuously replaced builds from every push to `main`, compared by build hash rather than version. |
| `dev` | `releases/download/dev/manifest.json` | The newest `v3` prerelease. Prerelease builds default to this channel. |

A release built from a prerelease tag is published as a GitHub prerelease and
refreshes the `dev` release, which holds the latest signed manifest. How to
apply an update is on [Installation](/getting-started/installation/#updating).

`gproxy --version` prints the version, channel, build hash, and installation
kind:

```text
gproxy 3.0.0 (channel releases, build 4054fe4f94ea, installation standalone)
```

Release builds report channel `releases` or `dev` and installation
`standalone`, `android-apk`, or `container`. Source builds report
`development` and `source`.
