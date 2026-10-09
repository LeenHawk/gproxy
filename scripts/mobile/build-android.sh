#!/usr/bin/env bash
# Shared source build; channel policy and signing remain separate.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../reproducible-env.sh"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
distribution="${1:?usage: build-android.sh direct|fdroid|google-play|appgallery [universal|aarch64|x86_64]}"
arch="${2:-universal}"
case "$distribution" in
  direct|fdroid|google-play|appgallery) ;;
  *) echo "unsupported Android distribution: $distribution" >&2; exit 2 ;;
esac
case "$arch" in
  universal) targets=(aarch64-linux-android x86_64-linux-android); arches=(aarch64 x86_64); abis=(arm64-v8a x86_64) ;;
  aarch64) targets=(aarch64-linux-android); arches=(aarch64); abis=(arm64-v8a) ;;
  x86_64) targets=(x86_64-linux-android); arches=(x86_64); abis=(x86_64) ;;
  *) echo "unsupported Android architecture: $arch" >&2; exit 2 ;;
esac
export GPROXY_ANDROID_DISTRIBUTION="$distribution"
if [ "$distribution" = direct ]; then
  config="$(python3 - <<'PY'
import json, os, tomllib
version = os.environ.get('GPROXY_BUILD_VERSION') or tomllib.load(open('Cargo.toml', 'rb'))['workspace']['package']['version']
print(json.dumps({'version': version, 'bundle': {'active': True}}))
PY
)"
  export GPROXY_INSTALLATION_KIND=android-apk
else
  config="$(python3 scripts/mobile/version.py)"
  export GPROXY_BUILD_CHANNEL=release
  # Store packages must not contain the direct-distribution updater/trust root.
  unset GPROXY_UPDATE_PUBKEY
fi
export GPROXY_BUILD_VERSION="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["version"])' "$config")"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
source scripts/android/sdk.sh
export ANDROID_NDK_HOME="$(android_ndk_root)"
ranlib=("$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-ranlib)
test "${#ranlib[@]}" -eq 1 && test -x "${ranlib[0]}"
for target in "${targets[@]}"; do
  export "CMAKE_TOOLCHAIN_FILE_${target//-/_}=$root/scripts/cmake/android.cmake"
  export "RANLIB_${target//-/_}=${ranlib[0]}"
  export "BINDGEN_EXTRA_CLANG_ARGS_${target//-/_}=--target=${target}28"
done
# Multi-ABI builds select CMake's ABI from Cargo's per-target TARGET variable.
unset GPROXY_ANDROID_ABI
if [ "${GPROXY_ANDROID_FRONTEND_READY:-0}" != 1 ]; then pnpm --dir console build; fi
formats=(--apk)
if [ "$distribution" = google-play ]; then formats+=(--aab); fi
(
  cd crates/gproxy-host-tauri
  bash "$root/scripts/with-tauri-android-lib.sh" pnpm exec tauri android build --ci "${formats[@]}" --target "${arches[@]}" --config "$config" -- --locked
)

output="$root/dist/mobile/$distribution/$arch"
mkdir -p "$output"
apk_dir=crates/gproxy-host-tauri/gen/android/app/build/outputs/apk/universal/release
apk_name="$(python3 - "$apk_dir/output-metadata.json" <<'PY'
import json, sys
elements = json.load(open(sys.argv[1]))['elements']
if len(elements) != 1:
    raise ValueError('Expected one universal APK output')
print(elements[0]['outputFile'])
PY
)"
apk="$output/gproxy-$distribution-$arch.apk"
cp "$apk_dir/$apk_name" "$apk"
python3 scripts/mobile/verify-android.py "$apk" --distribution "$distribution" --abis "${abis[@]}"
"$(android_build_tool "$(android_sdk_root)" zipalign)" -c -P 16 4 "$apk"
if [ "$distribution" = google-play ]; then
  bundle_dir=crates/gproxy-host-tauri/gen/android/app/build/outputs/bundle/universalRelease
  bundles=("$bundle_dir/"*.aab)
  test "${#bundles[@]}" -eq 1 && test -f "${bundles[0]}"
  aab="$output/gproxy-google-play-$arch.aab"
  cp "${bundles[0]}" "$aab"
  python3 scripts/mobile/verify-android.py "$aab" --distribution "$distribution" --abis "${abis[@]}"
fi
if [ -n "${GPROXY_ANDROID_KEYSTORE:-}" ] && [[ "$distribution" != fdroid && "$distribution" != direct ]]; then
  "$(android_build_tool "$(android_sdk_root)" apksigner)" verify "$apk"
  if [ "$distribution" = google-play ]; then jarsigner -verify "$aab"; fi
else
  echo "Unsigned preparation output; signing is still required before installation/upload."
fi
echo "$output"
