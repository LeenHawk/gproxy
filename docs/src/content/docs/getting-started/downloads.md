---
title: Downloads
description: "Release assets for GPROXY: native installers, portable archives, the container image, edge bundles, checksums, provenance, and the signed update manifest."
---

Every release is published on the GitHub Releases page under a `v3.x.y` tag:

<https://github.com/LeenHawk/gproxy/releases>

[Code signing policy](/deployment/code-signing/): the SignPath Foundation
application is awaiting review. Existing downloads are not retroactively signed.

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
| `gproxy-windows-x86_64` | x86_64-pc-windows-msvc | `.zip` | MSIX (Store submission) |
| `gproxy-windows-aarch64` | aarch64-pc-windows-msvc | `.zip` | MSIX (Store submission) |
| `gproxy-android-x86_64` | x86_64-linux-android | `.zip` | `.apk` |
| `gproxy-android-aarch64` | aarch64-linux-android | `.zip` | `.apk` |

Linux GNU builds link glibc. The `-musl` builds are static. Windows builds
link the C runtime statically.

## Native Installers

| Package | Platform | What it does |
| --- | --- | --- |
| `.deb` | Debian and Ubuntu families | Installs `/usr/bin/gproxy`, a desktop launcher, and an XDG autostart entry. |
| `.dmg` | macOS 11 or later | A `GPROXY.app` bundle that runs the server in the background and opens the console. |
| `.msix` | Windows 10 version 2004 or later | Store-managed installation, Start menu launcher, private data, and Windows Startup task. Store publication is pending. |
| `.apk` | Android 9 (API 28) or later | A signed app with a foreground service, a launcher screen, and in-app updates. |

Behaviour, data locations, and log paths for each installer are on the
[Installation](/getting-started/installation/) page.

Windows MSI packaging has been replaced by Store submission MSIX packages.
Unsigned MSIX files stay in Actions artifacts; they are not public Release
downloads. Until Store certification is complete, use the Windows portable ZIP.

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

`<tag>` is the Git tag of the release. The workflow publishes GNU and `-musl`
variants for `linux/amd64`, `linux/arm64`, and `linux/riscv64` to GHCR.
Container images are not Release attachments. See
[Container](/deployment/docker/) for offline transfer, volumes, and environment.

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

New builds use GitHub's release asset `digest` (SHA-256) instead of separate
`.sha256` attachments. Read the digest for the exact release you downloaded:

```bash
gh api 'repos/LeenHawk/gproxy/releases/tags/v<VERSION>' \
  --jq '.assets[] | select(.name == "gproxy-linux-x86_64.zip") | .digest'
sha256sum gproxy-linux-x86_64.zip
```

Compare the local hash with the value after `sha256:`. For build provenance,
verify GitHub's signed artifact attestation:

```bash
gh attestation verify gproxy-linux-x86_64.zip -R LeenHawk/gproxy
```

Toolchain versions and resolved base-image digests are preserved in a separate
custom attestation; see [Build Provenance](/deployment/release-build/#build-provenance).
Older releases keep their existing checksum and provenance attachments.

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
