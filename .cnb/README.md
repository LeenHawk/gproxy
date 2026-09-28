# CNB CI and releases

The `dev` branch publishes the rolling `nightly` release. A matching `v4.*`
version tag publishes a beta or stable release. `api_trigger_ci` reruns checks;
`api_trigger_release` runs both CI and release packaging. Publication waits for
CI and every package job, and verifies every archive checksum before signing.

The target list remains `scripts/release-targets.json`. Each target has a
**CLI / server** stage and, where supported, a separate **Application** stage:

| Platform | CLI / server | Application |
| --- | --- | --- |
| Linux GNU x86_64 / ARM64 | `gproxy-linux-*.zip`, containing `gproxy` | `gproxy-tauri-linux-*.deb`, containing `gproxy-desktop` |
| Linux musl x86_64 / ARM64 | `gproxy-linux-*-musl.zip` | None |
| Windows x86_64 / ARM64 | `gproxy-windows-*.zip`, containing `gproxy.exe` | `gproxy-tauri-windows-*.exe`, NSIS installer for `gproxy-desktop.exe` |
| macOS x86_64 / ARM64 | `gproxy-macos-*.zip`, containing `gproxy` | `gproxy-tauri-macos-*.app.zip`, containing the ad-hoc signed `GPROXY.app` |
| Android x86_64 / ARM64 | `gproxy-android-*.zip` and legacy server-wrapper `.apk` (`io.github.leenhawk.gproxy`) | `gproxy-tauri-android-*.apk` (`dev.gproxy.desktop`) |

The CLI updater uses the bare target triple. Android Application updates use
the distinct `<target>-tauri-apk` manifest entries. The legacy server wrapper
uses `<target>-apk`. Application packages are never renamed into CLI packages.
Desktop Application installers remain separate downloads; this does not add
CLI-style executable replacement to the desktop host.

Linux uses matching native CNB nodes. Windows uses cargo-xwin and the Windows
SDK, with an x86_64 CLI smoke check under Wine. macOS uses cargo-zigbuild, a
pinned SDK and rcodesign. Cross-built macOS and Windows ARM64 packages cannot
be execution-tested on Linux; successful compilation and packaging do not
claim native runtime verification. Android retains the NDK and both existing
APK signing paths. CNB produces NSIS / app ZIP installers instead of the
GitHub workflow's MSIX / DMG installers. Microsoft Store submissions and
GitHub attestations remain specific to the GitHub workflow.

## Signing

The public trust root is `.cnb/update-public-key`. Private material is imported
only in signing/Android stages from:

`https://cnb.cool/LeenHawk/gproxy-secrets/-/blob/main/release.json`

That secret repository file contains `UPDATE_SIGNING_PRIVATE_KEY_B64`,
`UPDATE_SIGNING_PUBLIC_KEY_B64`, the four `ANDROID_SIGNING_*` variables, and
`GH_TOKEN` / `GITLAB_RELEASE_TOKEN` for mirror publication.
Its `allow_slugs` must contain only `LeenHawk/gproxy`, `allow_branches` must
allow `dev` and `v*`, and `allow_events` must allow `push`, `tag_push` and
`api_trigger_release`. CNB requires secret files to be edited on its website.
No private key or long-lived CNB token belongs in this repository.

## Update sources

The Console update page independently selects **GitHub / CNB** and
**dev / beta / release**. Saving persists both settings. The command line has
`--update-source github|cnb` (`GPROXY_UPDATE_SOURCE`). CNB-built binaries default
to CNB via `GPROXY_BUILD_UPDATE_SOURCE=cnb`; GitHub builds retain GitHub.
`GPROXY_UPDATE_MANIFEST_URL` remains an explicit URL override. Both sources
verify the same signing key.

CNB channel manifests are hosted under
`https://cnb.cool/LeenHawk/gproxy/-/releases/download/{nightly|beta|release}/manifest.json`.
Nightly assets have a commit prefix and the manifest is uploaded last, so an
older manifest continues to reference immutable packages during publication.
Intermediate build bundles use commit attachments with a three-day TTL.
GNU and musl container manifests are published to
`docker.cnb.cool/leenhawk/gproxy:{nightly|version-tag}[-musl]`.

## Mirrors

CNB builds once and publishes identical packages to GitHub, GitLab and CNB.
Each host receives its own signed manifest with that host's download URLs.
Container images are also mirrored to GHCR and GitLab Container Registry.
GitLab pipelines are disabled; push the source to GitHub and CNB. GitLab's
existing pull mirror keeps source refs synchronized without executing builds.
Windows cross-builds retain `+crt-static` and disable Tauri's conflicting CRT
linker overrides. Desktop builds omit the Android-only DLL. macOS code and
resource hashes are checked independently; ad-hoc signatures are not notarization.
