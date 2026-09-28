# GitHub source, GitLab CI, three release mirrors

GitHub `LeenHawk/gproxy` is the primary repository. GitLab
`leenhawk1/gproxy` pulls its refs, builds `.gitlab-ci.yml`, and sends pipeline
status back through its GitHub integration. A GitHub push/PR webhook requests
pull mirroring; neither this trigger nor publishing uses GitHub Actions.

The `dev` branch builds the rolling `nightly` release. Matching `v4.*` version
tags build beta/stable releases. Release jobs require a protected ref and do
not run for external pull requests. Publication waits for all checks and all
ten targets in `scripts/release-targets.json`, plus Edge/Cloudflare.

Each target builds **CLI / server** and **Application** separately:

| Platform | CLI / server | Application |
| --- | --- | --- |
| Linux GNU x86_64 / ARM64 | `gproxy-linux-*.zip` | `gproxy-tauri-linux-*.deb` |
| Linux musl x86_64 / ARM64 | `gproxy-linux-*-musl.zip` | None |
| Windows x86_64 / ARM64 | `gproxy-windows-*.zip` (`gproxy.exe`) | `gproxy-tauri-windows-*.msix` (`gproxy-desktop.exe`) |
| macOS x86_64 / ARM64 | `gproxy-macos-*.zip` (`gproxy`) | `gproxy-tauri-macos-*.dmg` (Tauri application) |
| Android x86_64 / ARM64 | `gproxy-android-*.zip` and legacy server-wrapper APK | `gproxy-tauri-android-*.apk` |

Android Application identity is `dev.gproxy.desktop`; the server wrapper is
`io.github.leenhawk.gproxy`. Their signed manifest targets are respectively
`<triple>-tauri-apk` and `<triple>-apk`. CLI archives use bare triples.
The desktop host does not acquire CLI-style executable replacement.

Linux uses matching native runners. macOS uses Apple SDKs/codesign and runs
the Intel CLI through Rosetta on an Apple Silicon runner. Windows uses MSVC
on a Windows runner; its ARM64 build cannot be execution-tested on the AMD64
host. Android uses the NDK and the existing separate APK packaging paths.

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

GNU and musl multi-architecture container images are pushed to `ghcr.io/leenhawk/gproxy`,
`registry.gitlab.com/leenhawk1/gproxy`, and `docker.cnb.cool/leenhawk/gproxy`.

All mirrored binaries default to GitHub for updates. The Console independently
selects and saves GitHub/CNB and dev/beta/release. The CLI also accepts
`--update-source github|cnb` (`GPROXY_UPDATE_SOURCE`). An explicit
`GPROXY_UPDATE_MANIFEST_URL` overrides the source/channel URL.

## Project configuration

The GitLab pull mirror targets `https://github.com/LeenHawk/gproxy.git`,
with mirror-triggered pipelines enabled. `dev` and `v4.*` are protected.
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
