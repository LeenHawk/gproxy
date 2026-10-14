# Native OpenHarmony / HarmonyOS NEXT

The CLI targets `aarch64-unknown-linux-ohos` and `x86_64-unknown-linux-ohos`.
These are native OHOS binaries, separate from Android packages. Rust names
this platform `target_os = "linux", target_env = "ohos"`; Linux desktop
services and dependencies must not be selected merely from `target_os`.

The application HAP also targets ARM64 and x86_64 (including x86_64 simulators).
The application reuses `gproxy-host-tauri`, its existing IPC operations,
Console, instance setup and in-process data plane. It uses Tauri's experimental
`feat/open-harmony` port. `tauri-pins.json` records exact Tauri, Wry, Tao,
cargo-mobile2 and Ability commits from the shared runtime baseline. `ability-har.json` pins a beta.7 HAR source whose Rust crate
tree must match that Ability revision. `prepare-har.py` packages the HAR from
source, including DOM Storage support, rather than using the CLI template's
beta.0 package. The old HAR called `init()` without the context containing the
module name and private files directory, which prevents GPROXY startup.
This is not stable Tauri platform support.

System fonts are the default. After an explicit download, the read-only
`gproxy-fonts` protocol serves verified fonts from the application data directory.
The protocol is registered with ArkWeb before NativeAbility starts. Font license
notices remain bundled.

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

Public HAP builds are unsigned by default. Both `direct` and `appgallery` builds
accept `GPROXY_OHOS_SIGNING_CONFIG`, the path to one DevEco/Hvigor signing-config
entry with its certificate, signing profile and keystore stored outside the
checkout. `package-hap.py` collects the signed HAP when this configuration is
provided; it does not silently fall back to the unsigned artifact.
Without a configuration, HAP files must be signed before installation. Signing
alone does not grant restricted background permissions: the profile and device
policy determine the actual runtime grant.
Compilation and HAP inspection are not device execution tests. No HAP self-update path is implemented.

HAP builds emit only the `cdylib` and use fat LTO with one codegen unit.
After the Rust callback, the Hvigor hook strips staged native debug data while
preserving the ordinary ELF layout. OHOS's dynamic loader maps the ELF section
table; generic UPX shared-library packing removes it and can fail with `Invalid
argument` before native exports initialize. HAP and AppGallery collectors reject
libraries without valid section tables. AppGallery APP assembly reuses these
stripped, unpacked libraries. `configure-hap.py` enables `compressNativeLibs`
for HAP and AppGallery builds: native libraries are ZIP-compressed during
packaging and extracted intact at installation. This preserves the ELF payload
and section table. CLI executables still use UPX compression.

The native Ability supplies the app's private data directory. The existing
explicit private-file secret fallback is used because keyring has no OHOS
backend. CLI service registration is not available. The generated backup
extension is removed so instance data and credentials are not enrolled in
automatic backup.

## Signing a downloaded HAP

Install Python's `json5` dependency and use the SDK's `hap-sign-tool.jar` with
one external DevEco/Hvigor signing configuration. This also works without a
Rust/ArkTS rebuild:

```sh
python3 scripts/ohos/sign-hap.py gproxy.hap gproxy-signed.hap \
  --config /path/to/signing.json5 \
  --tool /path/to/sdk/toolchains/lib/hap-sign-tool.jar \
  --background
```

The signer prompts for passwords rather than putting them in process arguments.
The helper updates only background declarations in `module.json`, preserves
native library bytes, signs the package and verifies its cryptographic signature.
It does not generate a new system trust root or alter an already signed profile.

For offline OpenHarmony debug signing, the helper can also create and sign a
30-day debug profile from the SDK template, using a separate profile-signing
configuration. Both configurations use `material.keyAlias`, `material.certpath`,
`material.storeFile` and optional `material.signAlg`; the application config needs
`material.profile` only when an existing signed profile is used. The profile
config must identify a profile-signing key/certificate, not the application key.

```sh
python3 scripts/ohos/sign-hap.py gproxy.hap gproxy-debug-signed.hap \
  --config /path/to/application-signing.json5 \
  --tool /path/to/sdk/toolchains/lib/hap-sign-tool.jar \
  --background \
  --debug-template /path/to/sdk/toolchains/lib/UnsgnedDebugProfileTemplate.json \
  --profile-config /path/to/profile-signing.json5 \
  --udid YOUR_DEVICE_UDID
```

Get the target UDID with `hdc shell bm get -u`; repeat `--udid` for more devices.
The helper sets the profile's bundle identity and development certificate to
match the HAP and signing configuration, and adds the background ACL before
signing the profile. It never rewrites an already signed profile. Keep keys and
configuration files outside Git; use your device's accepted debug signing chain.
The SDK's public example keys are for local debugging, not publisher identity.

This local profile path follows the official
[OpenHarmony ACL debugging guide](https://github.com/openharmony/docs/blob/master/en/application-dev/security/AccessToken/declare-permissions-in-acl.md).
Commercial HarmonyOS devices may require a different trusted profile; an
OpenHarmony debug signature is not proof of acceptance there. Developer mode
alone is not treated as a permission grant. The settings switch reports the
actual `startBackgroundRunning` result on the target device.

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
- **Continuous tasks:** the settings switch detects the continuous-task system
  capability and calls `startBackgroundRunning(TASK_KEEPING)` directly. It does
  not disable developer/debug configurations using a hard-coded phone version or
  a separate token precheck. Success marks the task active; failure displays the
  system error. PC/2-in-1 defaults to restoring the task after the engine starts;
  phone/tablet activation is explicit. The HAP declares `KEEP_BACKGROUND_RUNNING`.
  The signing helper's `--background` adds `KEEP_BACKGROUND_RUNNING_SYSTEM` for
  a matching debug profile without rebuilding ArkTS; source builds can opt in
  with `GPROXY_OHOS_BACKGROUND_ACL=1`. These are declarations, not grants.
  Cancelling a task does not silently restart it.
- **Exit:** UIAbility destruction stops the engine and ends the process, matching
  Android's cold-restart semantics for the process-global instance.

The [startup API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-app-ability-autostartupmanager),
[status bar API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/statusbar-extension-manager),
and [continuous task API](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-resourceschedule-backgroundtaskmanager)
have separate device restrictions. A startup approval does not by itself grant
continuous background execution. Desktop Extension Kit's status bar is for
PC/2-in-1 devices; it is not the phone notification bar.

Validation covers the Rust bridge's N-API types, generated configuration,
Console interactions with a mocked native boundary and HAP execution on an
emulator. Emulator validation does not establish startup, native tray or
background survival on physical devices.

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

## Shared runtime update

The pins in this branch select the same Tauri 2.11.6, Wry and Ability runtime as
TauriTavern. Rust is compiled once by the CLI; the gproxy Hvigor hook only strips
staged libraries before signing. Application version constraints remain intact.

The toolchain image is pinned by digest in `.gitlab/Dockerfile.ohos` and records
its exact CLI repository/revision and dependency pins in
`/opt/ohos-tauri/cargo-tauri-source.json`. CLI 2.11.4 and Rust `tauri` 2.11.6
are separate package versions. The preparation step checks image/source pin
agreement, and both build entry points keep CLI version mismatch checks enabled.

The manual [OHOS runtime compatibility workflow](../../.github/workflows/ohos-runtime-check.yml)
builds unsigned ARM64 and x86_64 HAPs using the application release build path.
It accepts a toolchain image digest and uploads check artifacts without publishing
a release or updating release channels.
