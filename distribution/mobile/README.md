# Mobile store preparation

Targets: F-Droid, Google Play, Huawei AppGallery for Android, and native
HarmonyOS AppGallery, including planned mainland China and overseas distribution.
The developer accounts exist; the Google Play and Huawei applications have not
yet been created. This repository prepares packages and listing material. It
does not create store products, submit a review, or publish an application.

## Identity and versions

- Application/bundle name: `com.leenhawk.gproxy.app`; listing name: **GPROXY**.
- The graphical application uses this identifier on Android, HarmonyOS and all
  desktop platforms. The command-line edition already exists as **GPROXY CLI**.
  `com.leenhawk.gproxy.cli` is its corresponding project namespace. New macOS CLI service
  installations use this namespace; existing legacy services remain manageable
  under their registered label. The legacy Android wrapper is unchanged.
- Early v4 builds used `dev.gproxy.desktop`. The new mobile identity installs
  separately. Desktop default data directories, keychain service names and
  startup registration names also change. Disable old automatic startup and
  export configuration in the old application before switching, then import in
  the new one. Existing data and credentials are not deleted or auto-migrated.
- Preserve this identity when creating the Android and HarmonyOS applications
  in their respective consoles. Confirm availability before reserving it.
- `Cargo.toml` and `crates/gproxy-host-tauri/tauri.conf.json` must agree on the
  release version. Update `bundle.android.versionCode` in
  `distribution/mobile/tauri.store.conf.json` at the same time:
  `major * 1000000 + minor * 1000 + patch`. `scripts/mobile/version.py` checks it.
  This explicit code is a store-only override for F-Droid's regex extraction;
  ordinary direct builds keep Tauri's automatic version-code calculation.
- Initial Android store builds target **ARM64**. The optional x86_64 build is
  for testing or a separately planned distribution. If x86_64 is later added to
  F-Droid, give each ABI split its own version code. Google Play receives the ARM64 AAB.
- F-Droid normally signs with its own key. Google Play has an app-signing key and
  a separate upload key. AppGallery APKs use the publisher's signing key.
  Matching package names do not make differently signed APKs mutually updatable.
  Decide key continuity when creating the Play product. Do not overwrite or
  rotate an existing app's signing identity to make a submission pass.

## Android builds

Use Rust 1.98, Node 24, pnpm 9.15.9, Go 1.27.1, JDK 21, Android SDK 36,
build-tools 36.1.0 and NDK 30.0.15729638. Native dependencies also need clang,
libclang, CMake, pkg-config and Ninja. Put Go on PATH. Set `ANDROID_HOME` and
`ANDROID_NDK_HOME` to the installed tools.

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
bash scripts/mobile/build-android.sh fdroid
bash scripts/mobile/build-android.sh google-play
bash scripts/mobile/build-android.sh appgallery
```

Each script builds the Console and Rust library from source. Output is in
`dist/mobile/<channel>/aarch64/`. Google Play produces an AAB plus an APK for
inspection/testing; upload the AAB to Play. F-Droid and Android AppGallery use
APKs. The script checks package/version, a non-debuggable manifest, removal of
the APK updater and install permission, ELF 16 KiB alignment and APK ZIP alignment.
Store packages use ThinLTO and no UPX. A 16 KiB device/emulator test is still
required; binary alignment alone is not runtime validation.

`GPROXY_ANDROID_DISTRIBUTION` selects `fdroid`, `google-play` or `appgallery`.
Gradle removes the install permission, update Activity/provider and update
notification action. Rust excludes the JNI download entry point. Without this
environment variable, the ordinary direct-distribution build keeps its updater.

All application builds bundle their fonts and license notices for offline use.
Applications launch directly without requiring acceptance of the privacy notice.
Application settings include an offline **Privacy notice** dialog on all platforms.
Android store builds also provide a **Privacy** action in the gateway notification.
The notice is informational; closing it does not stop the gateway.

By default, packages are **unsigned preparation output**. For Play/AppGallery,
set these environment variables for a release build:

- `GPROXY_ANDROID_KEYSTORE`: absolute path to the keystore.
- `GPROXY_ANDROID_STORE_PASSWORD`, `GPROXY_ANDROID_KEY_ALIAS`.
- `GPROXY_ANDROID_KEY_PASSWORD`: optional; defaults to the store password.

Use the Google Play upload key for the Play build and the registered publisher
key for the AppGallery APK. F-Droid ignores these signing settings and must
receive an unsigned source-built APK. Keep signing files outside the checkout;
never put passwords or private keys in store listing files or issue comments.

## F-Droid submission

The recipe is `distribution/fdroid/com.leenhawk.gproxy.app.yml.in`. It uses pinned,
checksum-verified Node/Go/Gradle tools and the F-Droid `rustup` source library.
The Android UPX packer is built from the same pinned FLOSS source revision as
the direct-release packer, outside the scanned application checkout. Store
builds emit only the Android `cdylib` so Cargo actually applies fat LTO, with
one codegen unit; concurrency is controlled separately
with `CARGO_BUILD_JOBS`. Gradle strips native debug data and uses UPX
`--best --lzma --android-shlib` before APK/AAB assembly and signing, covering
F-Droid, Play, AppGallery and direct releases. APK alignment is checked afterwards.
Release APKs also use ZIP compression for native libraries, including APKs
generated from AABs. Android extracts those libraries during installation, so
the smaller download is accompanied by an extracted library copy on disk.
The scanner removes the repository's Gradle wrapper; the build step recreates a
launcher for the verified Gradle installation. There are no blanket scanner
exclusions. JavaScript dependencies are installed from frozen lockfiles during
the build. The recipe declares `NonFreeNet` for proprietary AI integrations.

To render metadata manually, select the exact published commit (`v4.0.3` or
newer; the earlier `v4.0.2` tag does not contain this build support):

```sh
python3 scripts/mobile/fdroid-metadata.py <public-commit-or-tag>
# Copy the generated dist/mobile/fdroid/metadata/com.leenhawk.gproxy.app.yml
# into your fdroiddata fork's metadata/ directory, then in that fork:
fdroid readmeta
fdroid lint com.leenhawk.gproxy.app
fdroid checkupdates --allow-dirty com.leenhawk.gproxy.app
fdroid build --server com.leenhawk.gproxy.app
```

Do not submit the `.yml.in` template or claim a local APK build is an F-Droid
isolated-server build. The full recipe, toolchain availability, source scanner
and resulting APK still need to pass fdroiddata CI. The recipe's `Binaries` URL
points to the signed reference APK published with each stable GitHub release;
`AllowedAPKSigningKeys` pins its signing certificate. F-Droid independently
rebuilds the application and verifies it against that reference APK.

### F-Droid automatic updates

After initial inclusion, F-Droid owns version tracking and build-recipe updates:

- `UpdateCheckMode: Tags ^v[0-9]+\.[0-9]+\.[0-9]+$` selects stable release tags,
  excluding the floating `nightly` and `staging` tags and prereleases.
- `UpdateCheckData` reads `versionCode` from
  `distribution/mobile/tauri.store.conf.json` and the version name from
  `crates/gproxy-host-tauri/tauri.conf.json` at that tag.
- `AutoUpdateMode: Version` reuses the latest accepted build recipe for the new
  version and selects the matching source revision.

Each stable release must increase the Android version code and publish
`gproxy-fdroid-aarch64.apk` at the URL declared by `Binaries`. The Release
workflow retains that signed reference APK build. GitHub does not submit a
separate F-Droid merge request for every release.

Initial inclusion and changes to the build recipe still require a manual
fdroiddata merge request. Until the initial MR is merged, F-Droid's updater
cannot track this application from its main metadata repository. Use the
metadata renderer above when a manual recipe update is needed, and follow the
upstream [App inclusion](https://gitlab.com/fdroid/fdroiddata/-/blob/master/.gitlab/merge_request_templates/App%20inclusion.md)
or [App update](https://gitlab.com/fdroid/fdroiddata/-/blob/master/.gitlab/merge_request_templates/App%20update.md)
checklist. F-Droid CI, maintainer approval, and repository publication remain
separate from publishing the upstream GitHub release.

## Native HarmonyOS AppGallery

Use a disposable checkout and the pinned toolchain from `.gitlab/Dockerfile.ohos`.
The OHOS overlay changes Cargo manifests, so never run it in a shared checkout.

```sh
pnpm --dir console install --frozen-lockfile
bash scripts/mobile/build-ohos.sh
```

The script builds the native Tauri/HarmonyOS application, then uses Hvigor's
`assembleApp` to collect an **APP upload bundle**, with embedded HAPs also copied
for device testing, under `dist/mobile/appgallery-ohos/`. The native library is
reused for the final APP packaging step; no released HAP is downloaded/repacked.
An unsigned HAP from the ordinary release workflow is not an upload-ready APP.

For signing, supply `GPROXY_OHOS_SIGNING_CONFIG`, the path to a JSON/JSON5 file
containing one valid release `signingConfigs` entry from your DevEco/Hvigor
configuration. Its `material.certpath`, `material.profile` and `material.storeFile`
must be absolute paths to the release certificate, distribution profile and
keystore. Use the exact application identity, key alias and credentials assigned
to your product. Without this file, output remains explicitly unsigned.

The store launcher shows the same offline notice using ArkUI before entering
the Rust Ability. This does not establish hardware compatibility. The port is
still experimental. Test startup, setup, the gateway, storage, exit, updates and
permission-denied behavior on supported Huawei devices before submission.

On API 20 phone/tablet devices, ordinary packages do not have continuous
`taskKeeping` background support. API 21+ phones/tablets require the restricted
`KEEP_BACKGROUND_RUNNING_SYSTEM` grant for this mode. `GPROXY_OHOS_BACKGROUND_ACL=1`
only declares that permission; it does not grant it. Leave it unset unless Huawei
approved the permission and the release profile carries the corresponding grant.
The listing must not promise continuous phone background operation without that
evidence. PC/2-in-1 behavior has separate platform support and needs its own tests.

## Listing and review material

- `listings.json`: English/Chinese shared text, with separate Android and native
  HarmonyOS paragraphs. Review the platform-specific paragraph when copying.
- `privacy/`: the bundled notices. `scripts/mobile/sync-listings.py` generates
  `fastlane/metadata/android/` text and the website's English/Chinese privacy pages.
- `fastlane/metadata/android/<locale>/images/`: application icon and feature
  graphic derived from the existing logo. Phone screenshots must come from the
  actual application. Never include private credentials or present illustrative metrics as measured results.
- Draft `changelogs/4000002.txt` files now cover English, Simplified Chinese and
  Traditional Chinese. Confirm the actual first-submission version before using
  them. They describe the candidate build, not an already published store release.
- Both privacy pages were deployed from the separate main-branch documentation
  commit `f04225b04` and verified publicly accessible on 2026-10-02:
  [English](https://gproxy.leenhawk.com/legal/privacy/) and
  [Simplified Chinese](https://gproxy.leenhawk.com/zh-cn/legal/privacy/).
- [media-guide.md](media-guide.md) describes the v4 browser-captured screenshot
  candidates and subtitled feature walkthroughs. They use demonstration API
  fixtures and are not Android/HarmonyOS device captures or foreground-service
  certification videos. Generated files live in `dist/mobile/listing-materials/`.
- `review-checklist.md` and `data-safety.md` capture the remaining account,
  jurisdiction, permission and disclosure work. They are worksheets, not filed
  declarations or a guarantee of approval.

The manual **Prepare mobile store packages** workflow builds unsigned Android
and HarmonyOS artifacts from the selected branch. It uploads only workflow
artifacts (which may be downloadable by users with access to public-repository
Actions). It does not submit to a store or update release/beta/dev channels.
Signed distribution builds and first product submissions remain separate steps.
The workflow becomes available for dispatch after its file reaches the repository's
default branch; merging preparation into `dev` alone does not publish or run it.

## Official references

- [F-Droid quick start](https://f-droid.org/zh_Hans/docs/Submitting_to_F-Droid_Quick_Start_Guide/)
- [F-Droid inclusion policy](https://f-droid.org/en/docs/Inclusion_Policy/)
- [Google Play app setup and packages](https://support.google.com/googleplay/android-developer/answer/9859152)
- [Google Play external-update restriction](https://support.google.com/googleplay/android-developer/answer/9888379)
- [Google Play foreground-service declarations](https://support.google.com/googleplay/android-developer/answer/13392821)
- [Google Play new personal-account testing](https://support.google.com/googleplay/android-developer/answer/14151465)
- [Huawei AppGallery](https://developer.huawei.com/consumer/cn/appgallery/)
- [Huawei application release documentation](https://developer.huawei.com/consumer/cn/doc/app/agc-help-releaseapp-0000001100306599)
- [HarmonyOS application publication](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-publish-app)

Use the authenticated store consoles for current region/category requirements.
The Huawei documentation site uses a dynamic frontend; its detailed current
review text was not retrievable during this preparation.

## Validation recorded on 2026-10-02

- Google Play ARM64 release APK/AAB builds and APK policy/alignment checks passed.
  An x86_64 AppGallery variant also built and installed with a temporary test key;
  that key is not a distribution identity.
- F-Droid [MR !50921](https://gitlab.com/fdroid/fdroiddata/-/merge_requests/50921)
  submitted v4.0.3. [Pipeline 2906649715](https://gitlab.com/LeenHawk/fdroiddata/-/pipelines/2906649715)
  passed all nine jobs, including the full F-Droid build, public-tag update check,
  metadata checks, and source/APK scans. Acceptance and publication remain pending;
  CI success does not establish device/runtime correctness.
- Console TypeScript, ESLint and translation checks passed. Python/shell syntax,
  Rust formatting and generated OHOS launcher/permission configuration were checked.
- The API 35 software emulator displayed the offline Android notice. Continuing
  reached MainActivity, but ANRs and a WebView renderer crash prevented completion
  of the application interaction test. System UI also experienced ANRs without
  KVM acceleration. The cause is not established; hardware-accelerated/device
  testing and real storefront screenshots remain required. Do not count this as
  a passed startup or privacy-settings interaction test.
- The new HarmonyOS store path has not been compiled with its SDK or tested on
  a device. Signing, APP output compatibility and ArkUI behavior remain unverified.
- Opus reviewed the user-facing English/Chinese text and the submission notes;
  its follow-up approved merging preparation material after revisions. That is a
  text review, not store certification or code/runtime validation.
