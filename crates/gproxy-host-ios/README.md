# Native iOS proxy sessions

This is an initial iOS 26 host, using SwiftUI, `BGContinuedProcessingTask`, and
the existing Rust gproxy instance and Axum router. It creates no VPN, modifies
no system routes, and requires no MDM. Clients explicitly use
`http://127.0.0.1:8787` and the displayed API key; arbitrary traffic from other
apps is not intercepted. The caller must support a custom API endpoint and
permit local HTTP connections.

## Session lifecycle

1. The user taps Start in the foreground (900, 3600, or 10000 seconds).
2. A continued-processing request uses `.fail`, not delayed queueing. The
   listener starts only after the scheduler launches the task.
3. Swift displays the endpoint, API-key copy action, incoming request count,
   remaining time, and a WKWebView for the existing authenticated Console.
4. User Stop, task expiration/cancellation, native listener failure, or the
   duration limit closes the listener and all owned connections and shuts
   down synchronization. A new session can then open the same database.

The native host also enforces the deadline, independently of the UI timer.
An expired session interrupts active streams. There is no automatic renewal,
silent restart, location/audio keepalive, or pretend countdown-based progress.
The workload reports indeterminate progress and actual incoming request
counts, not claims that requests or streamed responses have completed.

**The duration is an application limit, not a runtime grant from iOS.** Apple
can end a task earlier. Apple's examples cover user-initiated work that can
complete; they do not explicitly approve an idle local proxy awaiting future
requests. This implementation is an experiment, not evidence of App Store
acceptance or reliable lock-screen availability. Validate both before shipping.

## Build

Use macOS, Xcode 26 with the iOS 26 SDK, Rust, Node >=22.12, pnpm, XcodeGen,
CMake, and Go (required by the existing BoringSSL dependency).

```sh
bash scripts/build-ios.sh
# Apple silicon simulator, build this library as well before selecting it:
bash scripts/build-ios.sh aarch64-apple-ios-sim
open crates/gproxy-host-ios/ios/Gproxy.xcodeproj
```

Select a development team in Xcode. The generated project uses the existing
release-built Rust archive; rebuild it with the script after changing Rust.
Only ARM64 device and Apple silicon simulator targets are configured.
`UIBackgroundModes=processing` and the permitted wildcard task identifier are
in `Info.plist`. CPU/network processing needs no GPU entitlement and this app
does not request a Network Extension entitlement. Keep the task identifier in
the plist, controller, and bundle ID configuration consistent if rebranding.

## CI packages

The `iOS packages` GitHub Actions workflow runs on relevant `ios` branch pushes,
pull requests targeting `ios`, and manual dispatch. It builds the Console once, then uses
separate macOS 26 jobs to compile Rust and Swift for each SDK. No Apple secrets
are needed. Download these workflow artifacts:

| Artifact | Package | Use |
| --- | --- | --- |
| `gproxy-ios-arm64-device` | `gproxy-ios-arm64-unsigned.ipa` | Sign with a sideloading tool before installing on a device |
| `gproxy-ios-arm64-simulator` | `gproxy-ios-arm64-simulator.zip` | Extract the ad-hoc-signed `.app` and install on an ARM64 iOS 26 simulator |

Each package includes `SHA256SUMS`; Xcode build logs are uploaded separately.
The device IPA has the standard `Payload/Gproxy.app` layout but no Apple
signature or provisioning profile. It cannot be installed directly or uploaded
to TestFlight/App Store Connect. The simulator package is not a device IPA.
CI does not publish to GitHub Releases or any update channel.

Locally, run `bash scripts/package-ios.sh device` (or `simulator`) after building
the corresponding Rust target. To use an already-built Console, set
`GPROXY_IOS_SKIP_FRONTEND=1` and place its files in
`crates/gproxy-host-axum/assets/web/` before running `build-ios.sh`.

TauriTavern's adjacent iOS pipeline uses `tauri-action`, App Store Connect API
credentials, and signed IPA export before a separate TestFlight upload. This
host is native Swift rather than Tauri; the equivalent compile/archive and
artifact stages are implemented with `xcodebuild` and `ditto`. Signed export
and store submission are not configured here.

The Swift bridge serializes native operations off the main actor. Credentials
are generated once and stored in Keychain using
`AfterFirstUnlockThisDeviceOnly`; the database and files live in Application
Support with protection available after first unlock. Missing credentials for
an existing database fail startup instead of silently replacing the master
key. Upstream credentials use gproxy's existing sealed storage. Secrets are
never put in URLs or logs; copy actions deliberately put the selected secret
on the clipboard. Console login uses `admin` and the saved password.

## Validation

Linux can run `cargo test -p gproxy-host-ios --lib` and
`cargo clippy -p gproxy-host-ios --all-targets -- -D warnings`. These verify
the Rust host, not Swift compilation, scheduling, or iOS behavior.

On an iOS 26 device, verify:

- Start failure when background task execution is unavailable; no listener
  should be advertised or opened.
- An actual upstream request and SSE stream from a second app, both after
  switching apps and after locking/unlocking.
- Idle periods and accurate system task status; record any early termination.
- Stop via the app and the system task UI, deadline expiry, and restart in the
  same process. Verify established sockets are closed, not just new accepts.
- Force-quit, relaunch, and reboot; no automatic session should start.
- Existing WireGuard/Shadowrocket/Clash and enterprise VPN configurations,
  Wi-Fi/cellular transitions, and upstream reachability under their policies.

References: [Apple long-running tasks](https://developer.apple.com/documentation/backgroundtasks/performing-long-running-tasks-on-ios-and-ipados)
and [UIBackgroundModes](https://developer.apple.com/documentation/bundleresources/information-property-list/uibackgroundmodes).
