# The GPROXY Android application

English | [简体中文](ANDROID.zh-CN.md)

The same shell as the desktop, on a phone. One `App`, one IPC command table,
one console, one set of generated types — and a foreground service holding the
process up, because that is the only way Android lets a gateway keep running.

```
        the window                    an app on this device
            │                                   │
       Tauri IPC                        HTTP, 127.0.0.1:8787
            │                                   │
    ipc::table (261 commands)     gproxy-host-axum, data plane only
            └──────────────┬────────────────────┘
                     one gproxy_app::App
                 one Gproxy · one snapshot · one cache
                             │
                    GproxyService, foreground
              the only reason the process is still alive
```

## The loopback data plane is the point

An app on the device points at `http://127.0.0.1:8787` and is talking to this
process. That is what makes a phone useful as a gateway at all.

**It still requires a key**, exactly as on the desktop and for exactly the same
reason: every process on the device can reach loopback, and an unauthenticated
data plane would let any of them spend the user's upstream quota. Same
authenticator, same admission, same `401`. `/admin/api` and `/portal/api`
answer `404` here as they do on the desktop — the management surfaces are the
window's, over IPC.

## What Tauri gives, and the three things it does not

Tauri v2 supports Android and supplies the project scaffolding, the WebView,
the IPC bridge and the packaging. It supplies none of the three behaviours a
gateway needs on a phone. All three existed in v3's hand-written Java, and all
three are ported rather than reinvented.

| | v3, Java | v4, Kotlin in `gen/android/` |
|---|---|---|
| foreground service | `scripts/android/GproxyService.java.in` | `GproxyService.kt` |
| boot receiver | `scripts/android/GproxyBootReceiver.java.in` | `GproxyBootReceiver.kt` |
| in-app update | `GproxyUpdateActivity.java.in`, `GproxyUpdateProvider.java.in` | `GproxyUpdateActivity.kt`, `GproxyUpdateProvider.kt` |

### The foreground service supervises nothing

This is the one real difference from v3, and it changes the shape of
everything else.

v3's service copied a `gproxy.bin` out of the APK's assets, set
`LD_LIBRARY_PATH`, started it as a **child process**, polled
`http://127.0.0.1:8787/admin` until it answered, and pumped the child's stdout
into a ring buffer. The app and the gateway were two processes and the
service's job was to supervise the other one.

Here the engine is compiled into `libgproxy_host_tauri.so` and the data plane
listens from inside the app's own process. There is no child to supervise, no
asset to unpack, no health poll to write, and no way for two processes to
disagree about which database they opened. The service exists for one reason:
**to stop Android killing the process the engine is already in.** An app with
nothing in the foreground is frozen and then killed within seconds of the user
switching away.

Two consequences worth knowing:

**Stop ends the process.** The instance is assembled once per process and Tauri
has already handed it to 261 commands as managed state, so there is no honest
"stop" that leaves them holding a shut-down instance. Stop closes the socket,
stops the background sync, and then ends the process. The next start is a cold
one — which is also the only "restart" whose behaviour matches a first launch.

**The service type is `specialUse`, not `dataSync`.** From Android 14 a
foreground service must declare a type; from Android 15 a `dataSync` service is
capped at six hours in any twenty-four. A gateway that stops answering after
six hours is the same bug as one that stops when you switch apps. `specialUse`
has no such cap and carries a stated subtype
(`R.string.gproxy_special_use_subtype`). `dataSync` is declared as well, for
the releases that understand a type but not that one.

### The boot receiver

`BOOT_COMPLETED` needs `RECEIVE_BOOT_COMPLETED`; without the permission the
receiver is simply never called, silently. `MY_PACKAGE_REPLACED` is filtered
too, and matters as much: without it every update would be an outage until
somebody happened to open the app.

The receiver starts the service and returns. It never touches the engine — a
broadcast receiver has about ten seconds of main thread before Android declares
it stuck, and opening a cold database can take longer.

### The in-app update

`REQUEST_INSTALL_PACKAGES` in the manifest makes the install intent legal. The
per-app "install unknown apps" toggle is a separate, user-granted thing that
`GproxyUpdateActivity` asks for with `ACTION_MANAGE_UNKNOWN_APP_SOURCES` at the
moment it needs it; an install intent fired without it is silently refused.

`GproxyUpdateProvider` exposes exactly one read-only `content://` path, because
a file in this app's private storage is unreadable by the system installer and
a `file://` URI aimed at it has been a `FileUriExposedException` since
Android 7.

The foreground notification's **Update** action opens the updater. A worker
calls the same Rust manifest and artifact verification used by `gproxy update`:
ed25519 signature, channel, data-layout floor, size and SHA-256. It stages
`gproxy-update.apk` and then `install-apk.pending`; only that completed pair is
handed to Android's installer. Nothing installs on a schedule.

The manifest entry is `<target-triple>-tauri-apk`, separate from the legacy
`-apk` entries: those use a different application id. Supply
`gproxy-tauri-android-aarch64.apk` (or `gproxy-tauri-android-x86_64.apk`) and its
`.sha256` beside the native artifacts to include it in
`scripts/build-update-manifest.sh`. Build with `GPROXY_UPDATE_PUBKEY` and, for
rolling builds, `GPROXY_BUILD_CHANNEL=dev` and `GPROXY_BUILD_HASH`. The APK must
be signed with the same Android signing key as the installed app. The Release workflow builds both app architectures and supplies their signed
APKs to the manifest job. A manifest without the app artifact reports it
unavailable instead of offering the old wrapper APK.

## Why the process, and not the window, is the gateway

This arrangement is only legal because of something in Tauri's generated code,
so it is worth stating with the citation:
`gen/android/app/src/main/java/dev/gproxy/desktop/generated/WryActivity.kt`
calls `Rust.create()` — the call that runs `gproxy_host_tauri::start` — from
**`ProcessLifecycleOwner`**, not from the activity:

```kotlin
object WryLifecycleObserver : DefaultLifecycleObserver {
    override fun onCreate(owner: LifecycleOwner) {
        Rust.create()
        Rust.wryCreate()
    }
}
```

The Rust main therefore runs **once per process**. A foreground service holding
the process open after the window is destroyed is a supported state, and a
later activity re-attaches through `onActivityCreate` rather than starting a
second engine. That is why there is no re-entry guard in this crate: adding one
would be guarding against something the framework already prevents.

It is also why `engine::ensure_started` exists. The window is not the only way
in — the boot receiver has no window — so the instance is a process-global
assembled by whoever asks first, and the runtime moved out of `run()` for the
same reason: a runtime dropped when `run()` returns would take the listener and
the background sync with it.

## Secrets on a phone

`keyring` has no Android backend. Every call returns `Invalid("platform", …)`,
which `secrets` already reads as "there is no keychain here" — so the
`SecretStore` abstraction needed no change at all, and the behaviour is the one
already documented for a desktop with no keyring daemon:

| Secret | On Android |
|---|---|
| master key | **not minted**; upstream credentials are stored in the clear |
| gateway key | a `0600` file in the data directory |

`desktop_instance_status` reports `secretsAreSealed: false`, the service's
notification says so, and `gproxy::rotate::PLAINTEXT_SECRETS` is logged in the
same words the server uses.

Android's per-application UID makes this weaker than the same situation on a
desktop — the data directory is unreadable by *other apps*, rather than merely
by other users — but it is not nothing: `adb`, a backup agent and root all read
it. **Wrapping the master key in the Android Keystore through a second
`SecretStore` implementation is the named follow-up**, and the trait is why
that will be one new file rather than a change to eight call sites.

## What it requires

| | Version used here |
|---|---|
| Android SDK | platform `android-36`, build-tools 36.1.0 |
| NDK | 30.0.15729638 (`NDK_HOME` must point at it) |
| JDK | 21 |
| Gradle | 8.14.3, fetched by the wrapper in `gen/android/` |
| Rust targets | `aarch64-linux-android` and the other three ABIs |
| Tauri CLI | 2.11.5, pinned in `package.json` |

The Tauri CLI is a **dev dependency of this crate**, not a global tool:

```sh
cd crates/gproxy-host-tauri
pnpm install
```

`gen/android/buildSrc/.../BuildTask.kt` calls it back at
`node_modules/@tauri-apps/cli/tauri.js`, so a Gradle build and a `pnpm` build
are the same version of the tool. **That file is patched**: the generator
writes `listOf("tauri", …)`, which makes the call `node tauri …` and fails
because there is no file called `tauri` in the crate. If `tauri android init`
is ever re-run, re-apply the patch — it is commented in place.

## Building

```sh
cd crates/gproxy-host-tauri
export NDK_HOME=/path/to/Android/Sdk/ndk/30.0.15729638
export ANDROID_NDK_HOME="$NDK_HOME"
# NDK 30 requires a versioned target when bindgen reads its headers.
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android=--target=aarch64-linux-android28
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_linux_android=--target=x86_64-linux-android28
export BINDGEN_EXTRA_CLANG_ARGS_armv7_linux_androideabi=--target=armv7a-linux-androideabi28
export BINDGEN_EXTRA_CLANG_ARGS_i686_linux_android=--target=i686-linux-android28

pnpm android:build:arm64          # release APK, arm64 only
pnpm exec tauri android build --apk               # all four ABIs
pnpm exec tauri android build --apk --debug       # debug
```

A debug `.so` for this engine carries about a gigabyte of symbols; the release
profile (`opt-level = "z"`, `lto = "fat"`, `strip = "symbols"`) brings it to
about 43 MB. Build release unless you need a debugger.

## The APK, read back

`aapt dump badging` on the release build, with the ninety-odd translated
`application-label-*` lines cut:

```
package: name='dev.gproxy.desktop' versionCode='4000000' versionName='4.0.0' platformBuildVersionName='16' platformBuildVersionCode='36' compileSdkVersion='36' compileSdkVersionCodename='16'
sdkVersion:'28'
targetSdkVersion:'36'
uses-permission: name='android.permission.INTERNET'
uses-permission: name='android.permission.FOREGROUND_SERVICE'
uses-permission: name='android.permission.FOREGROUND_SERVICE_SPECIAL_USE'
uses-permission: name='android.permission.FOREGROUND_SERVICE_DATA_SYNC'
uses-permission: name='android.permission.POST_NOTIFICATIONS'
uses-permission: name='android.permission.RECEIVE_BOOT_COMPLETED'
uses-permission: name='android.permission.REQUEST_INSTALL_PACKAGES'
uses-permission: name='android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS'
uses-permission: name='dev.gproxy.desktop.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION'
application-label:'GPROXY'
application: label='GPROXY' icon='res/9w.png'
launchable-activity: name='dev.gproxy.desktop.MainActivity'  label='GPROXY' icon=''
leanback-launchable-activity: name='dev.gproxy.desktop.MainActivity'  label='GPROXY' icon='' banner=''
feature-group: label=''
  uses-feature-not-required: name='android.software.leanback'
  uses-feature: name='android.hardware.faketouch'
  uses-implied-feature: name='android.hardware.faketouch' reason='default feature for all apps'
main
other-activities
other-receivers
other-services
supports-screens: 'small' 'normal' 'large' 'xlarge'
supports-any-density: 'true'
native-code: 'arm64-v8a'
```

`aapt dump badging` will not name a service or a receiver, so the components
come from `aapt2 dump xmltree --file AndroidManifest.xml`:

```
E: service (line=99)
  android:name="dev.gproxy.desktop.GproxyService"
  android:exported=false
  android:stopWithTask=false
  android:foregroundServiceType=0x40000001
E: receiver (line=110)
  android:name="dev.gproxy.desktop.GproxyBootReceiver"
  android:exported=true
    E: action  android:name="android.intent.action.BOOT_COMPLETED"
    E: action  android:name="android.intent.action.MY_PACKAGE_REPLACED"
E: activity  android:name="dev.gproxy.desktop.GproxyUpdateActivity"  android:exported=false
E: provider  android:name="dev.gproxy.desktop.GproxyUpdateProvider"
  android:authorities="dev.gproxy.desktop.updates"  android:exported=false
```

`0x40000001` is `FOREGROUND_SERVICE_TYPE_SPECIAL_USE (0x40000000) |
FOREGROUND_SERVICE_TYPE_DATA_SYNC (0x1)`.

The native library, from `unzip -l`:

```
 42985744  1981-01-01 01:01   lib/arm64-v8a/libgproxy_host_tauri.so
```

46,607,735 bytes of APK, 919 entries. Only `arm64-v8a` because the build above
asked for one ABI; `--apk` with no `--target` produces all four.

R8 runs on the release build, and the JNI seam depends on class and method
names surviving it. From `mapping.txt`:

```
dev.gproxy.desktop.GproxyNative -> dev.gproxy.desktop.GproxyNative:
```

An identity mapping, so `Java_dev_gproxy_desktop_GproxyNative_nativeStart`
still resolves. `proguard-rules.pro` states the keep rules rather than relying
on the default file's.

## What is *not* verified

This machine has no attached device and no X server. **Nothing below has been
observed running.** Everything above is either source, or output read back out
of a built APK.

- **The application has never been launched.** Not on a device, not on an
  emulator. The window opening, the console rendering, an IPC command
  returning, the data plane answering on loopback — none of it has been seen on
  Android. All of it is exercised on the desktop by `tests/assembly.rs`, over
  the same code, which is evidence and not proof.
- **The foreground service has never posted a notification**, and the claim
  that the process survives backgrounding is a claim about Android's documented
  behaviour, not an observation.
- **The boot receiver has never fired.**
- **The in-app update has not been exercised on a device.** Download and
  verification are covered by a local signed-manifest test. The package installer,
  unknown-source permission screen and replacing an installed APK still need a
  device and matching signed releases.
- **The locally inspected release APK was unsigned.** `app-universal-release-unsigned.apk` is what
  Gradle produces without a signing config, and it cannot be installed as-is.
  The debug APK is signed with the local debug key. Release CI signs the APK
  after Gradle builds it.
- **The recorded local build covered only `arm64-v8a`.** The other three ABIs are configured and
  their Rust targets are installed, but no `armeabi-v7a`, `x86` or `x86_64`
  library has been compiled.
- **`catch_unwind` in the JNI functions catches nothing in a release build.**
  The workspace's release profile is `panic = "abort"`. The guard is real in a
  debug build, which is the one somebody runs when they are trying to find out
  why something panicked.
- **The battery-optimisation exemption dialog** is requested once and its
  effect is entirely vendor-dependent.

## Release CI

The `application` matrix in `.github/workflows/release.yml` builds ARM64 and
x86_64 APKs. `scripts/package-tauri-release.sh` aligns, signs and verifies each
APK with `zipalign` and `apksigner` after Gradle builds it. The job installs:

- the **Android SDK** with platform `android-36` and build-tools 36.1.0, and
  the licences accepted;
- the **NDK**, pinned — `NDK_HOME` is read by the Tauri CLI, and the NDK
  version determines the libc symbols the `.so` is linked against;
- a **JDK 21**;
- **Gradle** via the checked-in wrapper, with `~/.gradle` cached — an uncached
  first build downloads Gradle itself, the Android Gradle Plugin and the
  AndroidX dependencies;
- **`pnpm install`** in `crates/gproxy-host-tauri` for the pinned CLI;
- the selected **Rust Android target**, one ABI per job;
- **signing secrets** — `ANDROID_SIGNING_KEYSTORE_B64`,
  `ANDROID_SIGNING_KEYSTORE_PASSWORD`, `ANDROID_SIGNING_KEY_ALIAS`, and optional
  `ANDROID_SIGNING_KEY_PASSWORD`. Missing required secrets fail the release job.

Disk is the surprise: a debug `.so` for this engine is about a gigabyte, and
four ABIs of it will fill a small runner.

## v3's APK machinery is still here

Nothing Android has been deleted. `scripts/android/`,
`scripts/package-android-apk.sh` and the two `*-linux-android` release targets
all stay, and they are two different things:

- the **Java templates** are this port's source material, and the reference for
  anything that turns out to have been done for a reason not recorded here;
- the **`*-linux-android` targets** are Termux — the plain server binary on an
  Android ABI, run from a shell — which is a separate story from an app with a
  window, and unaffected by any of this.

v3's APK was package `io.github.leenhawk.gproxy`; this one is
`dev.gproxy.desktop`. They do not collide and can be installed side by side.

`dev.gproxy.desktop` is the desktop bundle's identifier too, and "desktop" in
an Android package name reads oddly. It is deliberate: one application, one
identity across both platforms, and it is the string `secrets::KEYCHAIN_SERVICE`
already files the desktop's keychain entries under. Two identifiers would be
two products.
