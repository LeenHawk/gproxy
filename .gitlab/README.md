# GitHub Actions, GitLab backup, three release mirrors

GitHub `LeenHawk/gproxy` is the primary repository and automatic CI/release
builder. GitLab `leenhawk1/gproxy` continues to pull its refs, but automatic
mirror pipelines are disabled by `.gitlab-ci.yml`. Start a backup pipeline
from GitLab's **Run pipeline** page or API on protected `dev`/`v4.*` refs.
Set `GPROXY_GITLAB_CI_ENABLED=true` only when intentionally restoring automatic
GitLab builds. Run one publisher at a time to avoid competing nightly updates.

The `dev` branch builds the rolling `nightly` release. Matching `v4.*` version
tags build beta/stable releases. Release jobs require a protected ref and do
not run for external pull requests. Publication waits for all checks and all
targets in `scripts/release-targets.json`, plus Edge/Cloudflare.

Each target builds **CLI / server** and **Application** separately:

| Platform | CLI / server | Application |
| --- | --- | --- |
| Linux GNU x86_64 / ARM64 / RISC-V 64 | ZIP, DEB | ZIP, DEB |
| Linux musl x86_64 / ARM64 / RISC-V 64 | ZIP, DEB | None |
| Windows x86_64 / ARM64 | ZIP, MSIX | ZIP, MSIX |
| macOS x86_64 / ARM64 | ZIP, DMG | ZIP, DMG |
| Android x86_64 / ARM64 | ZIP, Termux DEB | APK only |
| OpenHarmony x86_64 / ARM64 | ZIP | ARM64 experimental unsigned HAP |

CLI filenames use `gproxy-*`; Applications use `gproxy-tauri-*`. Linux CLI
packages install `gproxy` as `gproxy-cli`, distinct from the desktop package.
Android DEBs install under `/data/data/com.termux/files/usr` and bundle the
NDK C++ runtime privately; ZIPs keep their launcher and runtime alongside the
binary. The legacy Android server-wrapper APK is no longer built.

Windows CLI MSIX uses a separate `.CLI` identity and registers the `gproxy.exe`
console alias. macOS CLI DMGs contain the actual command and installation
instructions. Application ZIPs contain the Linux desktop payload, Windows
executable plus emitted DLLs, or the signed macOS `.app`. Linux ZIPs require
GTK/WebKitGTK and Windows ZIPs require WebView2 on the host.

Android Application identity is `com.leenhawk.gproxy.app`; its signed update
key remains `<triple>-tauri-apk`. CLI updates still use ZIPs and bare triples.
The desktop host does not acquire CLI-style executable replacement.

OHOS uses the pinned experimental Tauri port only in its own build checkout.
The shared `.gitlab/Dockerfile.ohos` image contains both SDKs and the Tauri/ohrs
build tools; GitHub reuses a content-addressed GHCR tag and GitLab retains its
registry layer cache. The HAP is explicitly unsigned and needs device/profile
signing before installation. No device execution or background-service support
is claimed. See `scripts/ohos/README.md` for the exact boundaries.

Edge publishes `gproxy-edge.wasm` and the deployable
`gproxy-edge-cloudflare.zip` bundle, with checksums and provenance.
In GitLab, native verification runs Clippy and tests once each over the whole workspace,
including Tauri, so both commands share the same enabled feature set. The
WASM compatibility and feature checks run in a parallel job. Both reuse the
prepared Linux toolchain image instead of installing tools on every run.
Release refs use their commit's image; other refs use the last prepared image
for the pinned Rust version. Native and WASM checks have separate target
directories and lockfile-keyed caches. Native caches omit test executables and
incremental compilation directories; dependency libraries and build-script
outputs remain cached, using fast ZIP extraction and compression.

GitLab Rust release compilation runs on 16-core AMD64 Linux runners. Six
registry-cached images provide GNU/musl cross tools and separate GTK development libraries for each Linux architecture,
Windows/macOS SDKs, and Android's NDK. CLI and Application use separate jobs and
caches. Linux ARM64 and RISC-V GNU use GCC and QEMU checks; musl (including RISC-V) and macOS use cargo-zigbuild;
Windows uses cargo-xwin. Android CLI uses the pinned official Termux builder
and the shared `distribution/termux/` recipe; Android Application uses Tauri's
Android build. CLI ZIPs and DEBs depend on Termux libraries and use package-manager updates.

Both CLI and desktop Application have ZIP and platform installer packages.
Windows SDK and macOS hdiutil jobs only seal already-cross-built
executables into MSIX/DMG; they do not compile Rust. macOS CLI signatures and
both CPU architectures are checked on the macOS packaging host, using Rosetta
for x86_64. App bundles are ad-hoc signed, not notarized.

UPX runs for supported Linux, Windows and Android binaries. Linux CLI binaries
run before and after compression in GitLab (QEMU for ARM64/RISC-V).
RISC-V CLI and Application use `--no-filter` with UPX 5.2.1 to avoid its
AUIPC filter failure. GNU provides the GTK/WebKit Application DEB; musl
provides the static CLI. GitHub cross-compiles RISC-V on AMD64 using the same
GitLab toolchain image and packaging script, including QEMU checks before
and after compression. Other GitHub targets retain their native runners. The uncompressed Windows
x64 CLI runs under Wine; the packed Wine check is informational because Wine
can reject UPX loaders. The Windows packaging job requires both uncompressed
and compressed x64 CLI executables to start successfully on Windows before
publication.
Windows ARM64 retains the patched UPX entry stub with fast NRV2E compression.
macOS Mach-O is left uncompressed. Windows ARM64 execution and desktop GUI
behavior are not exercised by these packaging checks.

CNB's automatic build entry point is disabled; it remains a source, release
and container mirror. The retained `.cnb/` scripts are a fallback, not the
active build path.

## Publishing

The same package, checksum and provenance files are uploaded to:

- `https://github.com/LeenHawk/gproxy/releases`
- `https://gitlab.com/leenhawk1/gproxy/-/releases`
- `https://cnb.cool/LeenHawk/gproxy/-/releases`

The signed manifest is generated separately for each host's download URLs,
using the same Ed25519 key. It is published after the packages it references.
Nightly package names include the source commit. GitLab hosts package bytes
in its Generic Package Registry and adds direct download links to Releases.
CNB receives the source ref before its release; it is an artifact mirror and
has no automatic build configuration in this checkout.

GNU and musl amd64/arm64/riscv64 container images are pushed to `ghcr.io/leenhawk/gproxy`,
`registry.gitlab.com/leenhawk1/gproxy`, and `docker.cnb.cool/leenhawk/gproxy`.

GitHub copies the complete container indexes to the other registries without
rebuilding. Its `release` environment also needs `GITLAB_RELEASE_TOKEN` and
`CNB_TOKEN`; signing and Android secrets remain in the same environment.

All mirrored binaries default to GitHub for updates. The Console independently
selects and saves GitHub/CNB and dev/beta/release. The CLI also accepts
`--update-source github|cnb` (`GPROXY_UPDATE_SOURCE`). An explicit
`GPROXY_UPDATE_MANIFEST_URL` overrides the source/channel URL.

## Project configuration

The GitLab pull mirror targets `https://github.com/LeenHawk/gproxy.git`,
with refs mirrored automatically; the workflow rules keep builds manual. `dev` and `v4.*` are protected.
The webhook/publisher project access token is restricted to this project;
renew it before its expiry date in GitLab project settings.

Required protected CI/CD variables:

- `UPDATE_SIGNING_PRIVATE_KEY_B64`, `UPDATE_SIGNING_PUBLIC_KEY_B64`, and the
  four `ANDROID_SIGNING_*` variables: environment scope `release/*`.
- `GH_TOKEN` (repository and packages write) and `GITLAB_RELEASE_TOKEN`
  (project API access): scope `release/publish`.
- `CNB_TOKEN`: a long-lived token for `LeenHawk/gproxy` code, releases and
  container packages. A short-lived interactive CLI login token is insufficient.
- The existing public `MS_STORE_*` application identity variables are needed
  for MSIX packaging. Store submission credentials are separate.

The public key compiled into packages is `.gitlab/update-public-key`. The
publish job checks that it matches the signing configuration. Private keys
must never be committed. GitHub-specific attestations, SignPath's GitHub
connector, and Microsoft Store submission are not run by this pipeline.
