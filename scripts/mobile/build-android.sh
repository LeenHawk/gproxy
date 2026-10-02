#!/usr/bin/env bash
# Source build for store distribution. No release APK is downloaded or repacked.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
distribution="${1:?usage: build-android.sh fdroid|google-play|appgallery [aarch64|x86_64]}"
arch="${2:-aarch64}"
case "$distribution" in
  fdroid|google-play|appgallery) ;;
  *) echo "unsupported store: $distribution" >&2; exit 2 ;;
esac
case "$arch" in
  aarch64) target=aarch64-linux-android ;;
  x86_64) target=x86_64-linux-android ;;
  *) echo "unsupported Android architecture: $arch" >&2; exit 2 ;;
esac
export GPROXY_ANDROID_DISTRIBUTION="$distribution"
config="$(python3 scripts/mobile/version.py)"
export GPROXY_BUILD_VERSION="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["version"])' "$config")"
export GPROXY_BUILD_CHANNEL=release
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
# A store build has no direct-distribution updater or signing trust root.
unset GPROXY_UPDATE_PUBKEY
source scripts/android/sdk.sh
export ANDROID_NDK_HOME="$(android_ndk_root)"
export GPROXY_ANDROID_ABI="$(android_abi "$target")"
export "CMAKE_TOOLCHAIN_FILE_${target//-/_}=$root/scripts/cmake/android.cmake"
ranlib=("$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-ranlib)
test "${#ranlib[@]}" -eq 1 && test -x "${ranlib[0]}"
export "RANLIB_${target//-/_}=${ranlib[0]}"
export "BINDGEN_EXTRA_CLANG_ARGS_${target//-/_}=--target=${target}28"

# Install dependencies first with --frozen-lockfile (see distribution/mobile/README.md).
VITE_GPROXY_BUNDLED_FONTS=1 pnpm --dir console build
formats=(--apk)
if [ "$distribution" = google-play ]; then formats+=(--aab); fi
(
  cd crates/gproxy-host-tauri
  bash "$root/scripts/with-tauri-android-lib.sh" pnpm exec tauri android build --ci "${formats[@]}" --target "$arch" --config "$config" -- --locked
)

output="$root/dist/mobile/$distribution/$arch"
mkdir -p "$output"
apk_dir=crates/gproxy-host-tauri/gen/android/app/build/outputs/apk/universal/release
apk_name="$(python3 - "$apk_dir/output-metadata.json" <<'PY'
import json, sys
elements = json.load(open(sys.argv[1]))["elements"]
if len(elements) != 1:
    raise ValueError("Expected one universal APK output")
print(elements[0]["outputFile"])
PY
)"
apk="$output/gproxy-$distribution-$arch.apk"
if [ "$distribution" = fdroid ]; then
  strip=("$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-strip)
  python3 scripts/pack-android-application.py "$apk_dir/$apk_name" "$output/packed.apk" "${strip[0]}"
  "$(android_build_tool "$(android_sdk_root)" zipalign)" -f -P 16 4 "$output/packed.apk" "$apk"
  rm "$output/packed.apk"
else
  cp "$apk_dir/$apk_name" "$apk"
fi
python3 scripts/mobile/verify-android.py "$apk"
"$(android_build_tool "$(android_sdk_root)" zipalign)" -c -P 16 4 "$apk"
if [ "$distribution" = google-play ]; then
  bundle_dir=crates/gproxy-host-tauri/gen/android/app/build/outputs/bundle/universalRelease
  mapfile -t bundles < <(find "$bundle_dir" -maxdepth 1 -name '*.aab')
  test "${#bundles[@]}" -eq 1
  cp "${bundles[0]}" "$output/gproxy-google-play-$arch.aab"
fi
if [ -n "${GPROXY_ANDROID_KEYSTORE:-}" ] && [ "$distribution" != fdroid ]; then
  "$(android_build_tool "$(android_sdk_root)" apksigner)" verify "$apk"
  if [ "$distribution" = google-play ]; then
    jarsigner -verify "$output/gproxy-google-play-$arch.aab"
  fi
else
  echo "Unsigned preparation output; signing is still required before installation/upload."
fi
echo "$output"
