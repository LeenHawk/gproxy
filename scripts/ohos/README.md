# Native OpenHarmony / HarmonyOS NEXT

The CLI targets `aarch64-unknown-linux-ohos` and `x86_64-unknown-linux-ohos`.
These are native OHOS binaries, separate from Android packages. Rust names
this platform `target_os = "linux", target_env = "ohos"`; Linux desktop
services and dependencies must not be selected merely from `target_os`.

The application HAP also targets ARM64 and x86_64 (including x86_64 simulators).
The application reuses `gproxy-host-tauri`, its existing IPC operations,
Console, instance setup and in-process data plane. It uses Tauri's experimental
`feat/open-harmony` port. `tauri-pins.json` records exact Tauri, Wry, Tao,
cargo-mobile2 and Ability commits, including the matching revisions from
upstream's lockfile. `ability-har.json` pins a beta.7 HAR source whose Rust crate
tree must match that Ability revision. `prepare-har.py` packages the HAR from
source, including DOM Storage support, rather than using the CLI template's
beta.0 package. The old HAR called `init()` without the context containing the
module name and private files directory, which prevents GPROXY startup.
This is not stable Tauri platform support.

## Build isolation

`prepare-tauri.py` modifies the Cargo manifests in the build checkout only.
Run it in a disposable CI checkout with `OHOS_TAURI_SOURCES` outside the
project workspace. Other platforms keep the normal `Cargo.toml` and
`Cargo.lock`. Never run this overlay in a shared development checkout.

CLI and HAP builds use the SDK from the shared
[tauri-harmony image](https://github.com/LeenHawk/tauri-harmony), pinned by digest
in `.gitlab/Dockerfile.ohos`. It contains checksum-verified Huawei Command Line
Tools 6.0.0 Release (6.0.0.858), including the API 20 SDK, hvigor and ohpm. Console builds use Node 24; the HAP tools use their bundled Node.

- [Official SDK release](https://github.com/openharmony/docs/blob/master/en/release-notes/OpenHarmony-v6.0.0.1-release.md)
- [Command-line tools mirror](https://github.com/ErBWs/ohos-sdk/releases/tag/6.0.0.858)
- [Tauri experimental branch](https://github.com/tauri-apps/tauri/tree/feat/open-harmony)

## Packaging and runtime boundaries

CLI ZIPs contain the UPX-packed `gproxy`, its stripped C++ runtime, and `run-gproxy.sh`. The native
binary remains named `gproxy` for the existing signed ZIP updater. Use
`sh ./run-gproxy.sh --help` from an OHOS device shell. SDK system-library
import stubs are never bundled. No Debian/Termux package is produced for OHOS.

HAP output is explicitly unsigned: no signing key or device profile is
available to this build. It must be signed for the intended device or
application distribution before installation. Compilation and HAP inspection
are not device execution tests. No HAP self-update path is implemented.

HAP builds emit only the `cdylib` and use fat LTO with one codegen unit.
After the Rust callback, the Hvigor hook strips staged native debug data while
preserving the ordinary ELF layout. OHOS's dynamic loader maps the ELF section
table; generic UPX shared-library packing removes it and can fail with `Invalid
argument` before native exports initialize. HAP and AppGallery collectors reject
libraries without valid section tables. AppGallery APP assembly reuses these
stripped, unpacked libraries. CLI executables still use UPX compression.

The native Ability supplies the app's private data directory. The existing
explicit private-file secret fallback is used because keyring has no OHOS
backend. CLI service registration is not available. The generated backup
extension is removed so instance data and credentials are not enrolled in
automatic backup.

## Application startup and background behavior

`application/EntryAbility.ets` extends the pinned RustAbility. Its N-API bridge
passes Application settings commands onto the ArkTS thread; the proxy remains
in-process. `configure-hap.py` installs these sources, native declarations, the
existing application icon, and the `taskKeeping` permission/mode in the generated
project. Phone, tablet and 2-in-1 devices share the HAP.

- **System startup:** users add GPROXY in Settings → Apps and meta services →
  App startup management. The UI opens `pc_app_setup_settings` in Huawei Settings
  and retains the manual path if the device cannot open it. This deep link requires
  HarmonyOS 6.0.0.112(SP3C00E101R12P6)+ where supported. The application never
  changes system startup approval itself.
- **Actual startup status:** `getAutoStartupStatusForSelf` is public on API 21+
  phones, tablets and PC/2-in-1 devices. The bridge resolves it on the running OS
  through N-API, retaining the API 20 build toolchain. Unavailable/failed queries
  are unknown, not disabled. Returning to the settings page refreshes the status.
- **PC status bar:** the native Desktop Extension Kit provides the icon and
  localized Open/Quit menu with listener status. Closing can hide the UIAbility
  after the status bar and continuous background task are available. Failure
  keeps the window accessible. Removing the tray changes closing to exit.
- **Automatic window behavior:** `LaunchReason.AUTO_STARTUP` distinguishes system
  startup from user activation. On PC, automatic startup can hide to a working
  status bar or minimize without a tray; manual activation shows the app. A
  `REQUIRED_HIDE` startup-page profile suppresses the system splash. Absence of
  all visible flashing has not been measured on hardware.
- **Continuous tasks:** `taskKeeping` is available on API 20 PC/2-in-1 devices;
  it defaults on there after the engine starts. On API 21+ phones/tablets it also
  requires the restricted `KEEP_BACKGROUND_RUNNING_SYSTEM` permission. Ordinary
  packages do not request that permission. A specially approved signing profile
  can opt into its declaration with `GPROXY_OHOS_BACKGROUND_ACL=1`; runtime checks
  still require an actual grant. Unsupported devices show the limitation and can
  continue to use the foreground app. The system notification opens the app, and
  cancelling the task is reflected in settings rather than silently restarted.
- **Exit:** UIAbility destruction stops the engine and ends the process, matching
  Android's cold-restart semantics for the process-global instance.

The [startup API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-app-ability-autostartupmanager),
[status bar API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/statusbar-extension-manager),
and [continuous task API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-resourceschedule-backgroundtaskmanager)
have separate device restrictions. A startup approval does not by itself grant
continuous background execution. Desktop Extension Kit's status bar is for
PC/2-in-1 devices; it is not the phone notification bar.

Local validation covers the Rust bridge's N-API types, generated configuration,
and Console interactions with a mocked native boundary. No signed HAP/device,
real startup, native tray or background-survival test has been performed locally.

GitHub Release and the GitLab fallback use `.gitlab/Dockerfile.ohos`.
That image adds GPROXY's Go build dependency and pinned UPX packer to the public toolchain.
SDK and experimental Tauri installation are maintained in the independent
`tauri-harmony` repository. `prepare-tauri.py` verifies its source pins and
applies the dependency overlay to the application checkout.
Application changes do not invalidate the toolchain image key. Cargo dependencies have a separate compiler cache.

The temporary probe has been removed after successful ARM64/x86_64 CLI ZIP
and ARM64 unsigned HAP builds on GitHub Actions. Release jobs retain the native
ELF identity/architecture and HAP library checks, and publish per-artifact
provenance. No compilation or SDK validation was performed locally.
