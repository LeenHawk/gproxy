# Native OpenHarmony / HarmonyOS NEXT

The CLI targets `aarch64-unknown-linux-ohos` and `x86_64-unknown-linux-ohos`.
These are native OHOS binaries, separate from Android packages. Rust names
this platform `target_os = "linux", target_env = "ohos"`; Linux desktop
services and dependencies must not be selected merely from `target_os`.

The application reuses `gproxy-host-tauri`, its existing IPC operations,
Console, instance setup and in-process data plane. It uses Tauri's experimental
`feat/open-harmony` port. `tauri-pins.json` records exact Tauri, Wry, Tao,
cargo-mobile2 and Ability commits, including the matching revisions from
upstream's lockfile. This is not stable Tauri platform support.

## Build isolation

`prepare-tauri.py` modifies the Cargo manifests in the build checkout only.
Run it in a disposable CI checkout with `OHOS_TAURI_SOURCES` outside the
project workspace. Other platforms keep the normal `Cargo.toml` and
`Cargo.lock`. Never run this overlay in a shared development checkout.

The CLI uses the official OpenHarmony 6.0.0.48/API 20 public SDK, verified by
Huawei's published SHA-256. HAP tooling uses a pinned, checksum-verified mirror
of Huawei Command Line Tools 6.0.0 Release (6.0.0.858), including hvigor and
ohpm. Console builds use Node 24; the HAP tools use their bundled Node.

- [Official SDK release](https://github.com/openharmony/docs/blob/master/en/release-notes/OpenHarmony-v6.0.0.1-release.md)
- [Command-line tools mirror](https://github.com/ErBWs/ohos-sdk/releases/tag/6.0.0.858)
- [Tauri experimental branch](https://github.com/tauri-apps/tauri/tree/feat/open-harmony)

## Packaging and runtime boundaries

CLI ZIPs contain `gproxy`, its C++ runtime, and `run-gproxy.sh`. The native
binary remains named `gproxy` for the existing signed ZIP updater. Use
`sh ./run-gproxy.sh --help` from an OHOS device shell. SDK system-library
import stubs are never bundled. No Debian/Termux package is produced for OHOS.

HAP output is explicitly unsigned: no signing key or device profile is
available to this build. It must be signed for the intended device or
application distribution before installation. Compilation and HAP inspection
are not device execution tests. No HAP self-update path is implemented.

The native Ability supplies the app's private data directory. The existing
explicit private-file secret fallback is used because keyring has no OHOS
backend. Tray, login startup and CLI service registration are not available.
The HAP runs as an application; no background-service or boot-restart support
is claimed. Its generated backup extension is removed so instance data and
credentials are not enrolled in automatic backup.

GitHub Release and the GitLab fallback use `.gitlab/Dockerfile.ohos`.
The SDK/source installation happens once in `prepare-toolchain.py` while
building the image; `prepare-tauri.py` only applies its dependency overlay to
the current application checkout. Application changes do not invalidate the
toolchain image key. Cargo dependencies have a separate compiler cache.

The temporary probe has been removed after successful ARM64/x86_64 CLI ZIP
and ARM64 unsigned HAP builds on GitHub Actions. Release jobs retain the native
ELF identity/architecture and HAP library checks, and publish per-artifact
provenance. No compilation or SDK validation was performed locally.
