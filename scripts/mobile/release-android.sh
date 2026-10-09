#!/usr/bin/env bash
# All Android application channels share one toolchain, frontend and build cache.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
source scripts/reproducible-env.sh
: "${GPROXY_BUILD_VERSION:?}"
: "${GPROXY_BUILD_CHANNEL:?}"
channel="$GPROXY_BUILD_CHANNEL"
source scripts/android/sdk.sh
# Match the F-Droid recipe's NDK path and scanned Gradle-wrapper replacement.
ln -sfn "$(android_ndk_root)" /tmp/gproxy-reproduce/android-ndk
export ANDROID_NDK_HOME=/tmp/gproxy-reproduce/android-ndk
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME" NDK_HOME="$ANDROID_NDK_HOME"
printf '#!/usr/bin/env bash\nexec gradle "$@"\n' > crates/gproxy-host-tauri/gen/android/gradlew
chmod +x crates/gproxy-host-tauri/gen/android/gradlew
pnpm --dir console install --frozen-lockfile
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
# Build once for every channel. F-Droid's independent builder runs the same build.
pnpm --dir console build
export GPROXY_ANDROID_FRONTEND_READY=1
mkdir -p dist/release
for arch in aarch64 x86_64; do
  export TARGET_TRIPLE="$arch-linux-android" TARGET_OS=android
  export ARTIFACT_NAME="gproxy-tauri-android-$arch"
  bash scripts/package-tauri-release.sh
  GPROXY_VERSION="$GPROXY_BUILD_VERSION" BUILDER=tauri UPX_ENABLED=true bash scripts/build-provenance.sh
done
# Exercise store packaging on dev before publishing a stable release.
if [[ "$channel" != release && "$channel" != dev ]]; then exit 0; fi

bash scripts/mobile/build-android.sh fdroid universal
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
signer="$(android_build_tool "$(android_sdk_root)" apksigner)"
: "${ANDROID_SIGNING_KEYSTORE_B64:?}"
: "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?}"
: "${ANDROID_SIGNING_KEY_ALIAS:?}"
(umask 077; printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$work/reference.jks")
options=(--ks "$work/reference.jks" --ks-pass env:ANDROID_SIGNING_KEYSTORE_PASSWORD --ks-key-alias "$ANDROID_SIGNING_KEY_ALIAS" --v1-signing-enabled false --v4-signing-enabled false)
if [ -n "${ANDROID_SIGNING_KEY_PASSWORD:-}" ]; then options+=(--key-pass env:ANDROID_SIGNING_KEY_PASSWORD); fi
reference=gproxy-fdroid-universal
unsigned="dist/mobile/fdroid/universal/$reference.apk"
"$signer" sign "${options[@]}" --out "dist/release/$reference.apk" "$unsigned"
# This checks signature-copy compatibility against this build, not a second rebuild.
python3 scripts/mobile/verify-reproducible-apk.py "dist/release/$reference.apk" "$unsigned" \
  --build-tools "$(android_sdk_root)/build-tools/34.0.0" \
  --expected-cert-sha256 80b7795d2d557bad6fba2a956771c91f77f1ceef5e2b536e45eee314a74eed1c \
  --report dist/mobile/fdroid/signature-report.json
(cd dist/release && sha256sum "$reference.apk" > "$reference.apk.sha256")
ARTIFACT_NAME="$reference" TARGET_TRIPLE=android-universal GPROXY_VERSION="$GPROXY_BUILD_VERSION" BUILDER=tauri UPX_ENABLED=true bash scripts/build-provenance.sh

# Play's upload key is separate from the public APK signing key. Never substitute it.
if [ -n "${GOOGLE_PLAY_UPLOAD_KEYSTORE_B64:-}" ]; then
  (umask 077; printf '%s' "$GOOGLE_PLAY_UPLOAD_KEYSTORE_B64" | base64 -d > "$work/play-upload.jks")
  export GPROXY_ANDROID_KEYSTORE="$work/play-upload.jks"
  export GPROXY_ANDROID_STORE_PASSWORD="${GOOGLE_PLAY_UPLOAD_STORE_PASSWORD:?}"
  export GPROXY_ANDROID_KEY_ALIAS="${GOOGLE_PLAY_UPLOAD_KEY_ALIAS:?}"
  export GPROXY_ANDROID_KEY_PASSWORD="${GOOGLE_PLAY_UPLOAD_KEY_PASSWORD:-$GPROXY_ANDROID_STORE_PASSWORD}"
else
  unset GPROXY_ANDROID_KEYSTORE GPROXY_ANDROID_STORE_PASSWORD GPROXY_ANDROID_KEY_ALIAS GPROXY_ANDROID_KEY_PASSWORD
  echo '::warning::Google Play upload key is not configured; the AAB requires signing before upload.'
fi
bash scripts/mobile/build-android.sh google-play universal
bundle=gproxy-google-play-universal
cp "dist/mobile/google-play/universal/$bundle.aab" "dist/release/$bundle.aab"
(cd dist/release && sha256sum "$bundle.aab" > "$bundle.aab.sha256")
ARTIFACT_NAME="$bundle" TARGET_TRIPLE=android-universal GPROXY_VERSION="$GPROXY_BUILD_VERSION" BUILDER=tauri UPX_ENABLED=true bash scripts/build-provenance.sh
