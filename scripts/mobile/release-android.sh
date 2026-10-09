#!/usr/bin/env bash
# Six channel/ABI builders export native inputs; one packaging job signs outputs.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
source scripts/reproducible-env.sh
: "${GPROXY_BUILD_VERSION:?}"
: "${GPROXY_BUILD_CHANNEL:?}"
mode="${1:?usage: release-android.sh build CHANNEL ARCH | package INPUTS}"
source scripts/android/sdk.sh
ln -sfn "$(android_ndk_root)" /tmp/gproxy-reproduce/android-ndk
export ANDROID_NDK_HOME=/tmp/gproxy-reproduce/android-ndk
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME" NDK_HOME="$ANDROID_NDK_HOME"
printf '#!/usr/bin/env bash\nexec gradle "$@"\n' > crates/gproxy-host-tauri/gen/android/gradlew
chmod +x crates/gproxy-host-tauri/gen/android/gradlew
pnpm --dir console install --frozen-lockfile
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
if [ -f console/dist/index.html ]; then
  node console/scripts/sync-to-embed.mjs
else
  pnpm --dir console build
fi
export GPROXY_ANDROID_FRONTEND_READY=1
if [ "$mode" = build ]; then
  distribution="${2:?channel}"; arch="${3:?architecture}"
  unset GPROXY_ANDROID_REUSE_NATIVE GPROXY_ANDROID_KEYSTORE
  GPROXY_ANDROID_APK_ONLY=1 bash scripts/mobile/build-android.sh "$distribution" "$arch"
  python3 scripts/mobile/android-native-artifact.py export "$distribution" dist/android-build --arch "$arch"
  exit 0
fi
if [ "$mode" != package ]; then echo "Unknown Android phase: $mode" >&2; exit 2; fi
inputs="${2:?native artifacts directory}"
# Generated Gradle settings refer to Cargo's Java/Kotlin sources. Fetching them
# does not compile Rust; the six producer jobs already built every native ABI.
cargo fetch --locked --target aarch64-linux-android --target x86_64-linux-android
mkdir -p dist/release
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
signer="$(android_build_tool "$(android_sdk_root)" apksigner)"
: "${ANDROID_SIGNING_KEYSTORE_B64:?}"
: "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?}"
: "${ANDROID_SIGNING_KEY_ALIAS:?}"
(umask 077; printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$work/reference.jks")
options=(--ks "$work/reference.jks" --ks-pass env:ANDROID_SIGNING_KEYSTORE_PASSWORD --ks-key-alias "$ANDROID_SIGNING_KEY_ALIAS" --v1-signing-enabled false --v4-signing-enabled false)
if [ -n "${ANDROID_SIGNING_KEY_PASSWORD:-}" ]; then options+=(--key-pass env:ANDROID_SIGNING_KEY_PASSWORD); fi
record() {
  local artifact="$1" extension="$2" target="$3"
  (cd dist/release && sha256sum "$artifact.$extension" > "$artifact.$extension.sha256")
  ARTIFACT_NAME="$artifact" TARGET_TRIPLE="$target" GPROXY_VERSION="$GPROXY_BUILD_VERSION" BUILDER=tauri UPX_ENABLED=true bash scripts/build-provenance.sh
}
python3 scripts/mobile/android-native-artifact.py import direct "$inputs"
for arch in aarch64 x86_64; do
  artifact="gproxy-tauri-android-$arch"
  apk="$inputs/direct/$arch/unsigned.apk"
  python3 scripts/mobile/verify-android.py "$apk" --distribution direct --abis "$(android_abi "$arch-linux-android")"
  "$signer" sign "${options[@]}" --out "dist/release/$artifact.apk" "$apk"
  "$signer" verify --verbose "dist/release/$artifact.apk"
  record "$artifact" apk "$arch-linux-android"
done

python3 scripts/mobile/android-native-artifact.py import fdroid "$inputs"
GPROXY_ANDROID_REUSE_NATIVE=1 bash scripts/mobile/build-android.sh fdroid universal
reference=gproxy-fdroid-universal
unsigned="dist/mobile/fdroid/universal/$reference.apk"
"$signer" sign "${options[@]}" --out "dist/release/$reference.apk" "$unsigned"
# Signature-copy compatibility of this build, not a second native rebuild.
python3 scripts/mobile/verify-reproducible-apk.py "dist/release/$reference.apk" "$unsigned" \
  --build-tools "$(android_sdk_root)/build-tools/34.0.0" \
  --expected-cert-sha256 80b7795d2d557bad6fba2a956771c91f77f1ceef5e2b536e45eee314a74eed1c \
  --report dist/mobile/fdroid/signature-report.json
record "$reference" apk android-universal

# Play's upload key is separate from the public APK signing key.
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
python3 scripts/mobile/android-native-artifact.py import google-play "$inputs"
GPROXY_ANDROID_REUSE_NATIVE=1 bash scripts/mobile/build-android.sh google-play universal
bundle=gproxy-google-play-universal
cp "dist/mobile/google-play/universal/$bundle.aab" "dist/release/$bundle.aab"
record "$bundle" aab android-universal
