#!/usr/bin/env bash
# Package a Unix release build into the zip shape published by release.yml.
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
target_os="${TARGET_OS:?missing TARGET_OS}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
binary="target/$target/release/gproxy"
android_package_base="${ANDROID_APK_PACKAGE_BASE:-io.github.leenhawk.gproxy}"

find_android_libcxx() {
  local target="$1"
  local ndk_root=""
  local candidate
  for candidate in "${ANDROID_NDK_ROOT:-}" "${ANDROID_NDK_HOME:-}"; do
    if [ -n "$candidate" ] && [ -d "$candidate" ]; then
      ndk_root="$candidate"
      break
    fi
  done

  if [ -z "$ndk_root" ]; then
    local sdk_root
    for sdk_root in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"; do
      if [ -n "$sdk_root" ] && [ -d "$sdk_root/ndk" ]; then
        ndk_root="$(find "$sdk_root/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
        break
      fi
    done
  fi

  if [ -z "$ndk_root" ] || [ ! -d "$ndk_root" ]; then
    echo "could not locate Android NDK; set ANDROID_NDK_ROOT or ANDROID_HOME" >&2
    exit 1
  fi

  local prebuilt="$ndk_root/toolchains/llvm/prebuilt"
  if [ ! -d "$prebuilt" ]; then
    echo "missing Android NDK LLVM prebuilt directory under $ndk_root" >&2
    exit 1
  fi

  local libcxx
  libcxx="$(find "$prebuilt" \
    -path "*/sysroot/usr/lib/$target/libc++_shared.so" \
    -type f | sort | head -1)"
  if [ -z "$libcxx" ] || [ ! -f "$libcxx" ]; then
    echo "could not locate libc++_shared.so for $target under $ndk_root" >&2
    exit 1
  fi
  printf '%s\n' "$libcxx"
}

write_android_launcher() {
  local out="$1"
  cat > "$out" <<'EOF'
#!/system/bin/sh
set -eu

self="$0"
case "$self" in
  */*) dir="${self%/*}" ;;
  *)
    resolved="$(command -v "$self" 2>/dev/null || true)"
    case "$resolved" in
      */*) dir="${resolved%/*}" ;;
      *) dir="." ;;
    esac
    ;;
esac

case "${LD_LIBRARY_PATH:-}" in
  "") export LD_LIBRARY_PATH="$dir" ;;
  *) export LD_LIBRARY_PATH="$dir:$LD_LIBRARY_PATH" ;;
esac

exec "$dir/gproxy.bin" "$@"
EOF
  chmod 755 "$out"
}

find_android_sdk_root() {
  local sdk_root
  for sdk_root in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"; do
    if [ -n "$sdk_root" ] && [ -d "$sdk_root" ]; then
      printf '%s\n' "$sdk_root"
      return 0
    fi
  done
  echo "could not locate Android SDK; set ANDROID_HOME or ANDROID_SDK_ROOT" >&2
  exit 1
}

find_android_platform_jar() {
  local sdk_root="$1"
  local android_jar
  android_jar="$(find "$sdk_root/platforms" -maxdepth 2 -name android.jar -type f 2>/dev/null | sort -V | tail -1)"
  if [ -z "$android_jar" ] || [ ! -f "$android_jar" ]; then
    echo "could not locate android.jar under $sdk_root/platforms" >&2
    exit 1
  fi
  printf '%s\n' "$android_jar"
}

find_android_build_tool() {
  local sdk_root="$1"
  local tool="$2"
  local path
  path="$(find "$sdk_root/build-tools" -maxdepth 2 -name "$tool" -type f 2>/dev/null | sort -V | tail -1)"
  if [ -z "$path" ] || [ ! -x "$path" ]; then
    echo "could not locate Android build tool '$tool' under $sdk_root/build-tools" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

android_abi_for_target() {
  case "$1" in
    aarch64-linux-android) printf '%s\n' "arm64-v8a" ;;
    x86_64-linux-android) printf '%s\n' "x86_64" ;;
    *) echo "unsupported Android target for APK packaging: $1" >&2; exit 1 ;;
  esac
}

android_package_suffix_for_target() {
  case "$1" in
    aarch64-linux-android) printf '%s\n' "arm64" ;;
    x86_64-linux-android) printf '%s\n' "x64" ;;
    *) echo "unsupported Android target for APK packaging: $1" >&2; exit 1 ;;
  esac
}

prepare_android_keystore() {
  local work="$1"
  local keystore="$work/signing.keystore"

  if [ -n "${ANDROID_SIGNING_KEYSTORE_B64:-}" ]; then
    : "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?missing ANDROID_SIGNING_KEYSTORE_PASSWORD}"
    printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$keystore"
    printf '%s\n' "$keystore"
    return 0
  fi

  keytool -genkeypair \
    -keystore "$keystore" \
    -storepass android \
    -keypass android \
    -alias androiddebugkey \
    -keyalg RSA \
    -keysize 2048 \
    -validity 10000 \
    -dname "CN=Android Debug,O=Android,C=US" >/dev/null
  printf '%s\n' "$keystore"
}

sign_android_apk() {
  local apksigner="$1"
  local work="$2"
  local aligned="$3"
  local signed="$4"
  local keystore
  keystore="$(prepare_android_keystore "$work")"

  local storepass="${ANDROID_SIGNING_KEYSTORE_PASSWORD:-android}"
  local keypass="${ANDROID_SIGNING_KEY_PASSWORD:-}"
  local alias="${ANDROID_SIGNING_KEY_ALIAS:-}"
  local signer_args=()
  if [ -z "$alias" ] && [ -z "${ANDROID_SIGNING_KEYSTORE_B64:-}" ]; then
    alias="androiddebugkey"
    keypass="android"
  fi
  if [ -n "$alias" ]; then
    signer_args+=(--ks-key-alias "$alias")
  fi
  if [ -n "$keypass" ]; then
    signer_args+=(--key-pass "pass:$keypass")
  fi

  "$apksigner" sign \
    --v4-signing-enabled false \
    --ks "$keystore" \
    "${signer_args[@]}" \
    --ks-pass "pass:$storepass" \
    --out "$signed" \
    "$aligned"
  "$apksigner" verify --verbose "$signed" >/dev/null
}

package_android_apk() {
  local version_code="${ANDROID_VERSION_CODE:-1}"
  local version_name="${ANDROID_VERSION_NAME:-0.0.0}"
  local suffix abi package_name sdk_root android_jar aapt zipalign apksigner api work
  suffix="$(android_package_suffix_for_target "$target")"
  abi="$(android_abi_for_target "$target")"
  package_name="$android_package_base.$suffix"
  sdk_root="$(find_android_sdk_root)"
  android_jar="$(find_android_platform_jar "$sdk_root")"
  aapt="$(find_android_build_tool "$sdk_root" aapt)"
  zipalign="$(find_android_build_tool "$sdk_root" zipalign)"
  apksigner="$(find_android_build_tool "$sdk_root" apksigner)"
  api="${android_jar%/android.jar}"
  api="${api##*/android-}"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN

  mkdir -p "$work/assets/gproxy" "$work/native/lib/$abi"
  cp dist/gproxy dist/gproxy.bin dist/libc++_shared.so README.md "$work/assets/gproxy/"
  cp dist/gproxy.bin "$work/native/lib/$abi/libgproxy_exec.so"
  cp dist/libc++_shared.so "$work/native/lib/$abi/libc++_shared.so"

  cat > "$work/AndroidManifest.xml" <<EOF
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="$package_name"
    android:versionCode="$version_code"
    android:versionName="$version_name">
    <uses-sdk android:minSdkVersion="23" android:targetSdkVersion="$api" />
    <application
        android:label="GPROXY"
        android:hasCode="false"
        android:extractNativeLibs="true"
        android:allowBackup="false"
        android:supportsRtl="true" />
</manifest>
EOF

  "$aapt" package \
    -f \
    -M "$work/AndroidManifest.xml" \
    -I "$android_jar" \
    -A "$work/assets" \
    -F "$work/unsigned.apk" >/dev/null
  (cd "$work/native" && zip -qr "$work/unsigned.apk" lib)
  "$zipalign" -f -p 4 "$work/unsigned.apk" "$work/aligned.apk"
  sign_android_apk "$apksigner" "$work" "$work/aligned.apk" "$artifact.apk"
  shasum -a 256 "$artifact.apk" > "$artifact.apk.sha256"
  trap - RETURN
  rm -rf "$work"
}

if [ ! -f "$binary" ]; then
  echo "missing release binary: $binary" >&2
  exit 1
fi

rm -rf dist
mkdir -p dist
cp README.md dist/

if [ "$target_os" = "android" ]; then
  cp "$binary" dist/gproxy.bin
  chmod 755 dist/gproxy.bin
  cp "$(find_android_libcxx "$target")" dist/libc++_shared.so
  chmod 644 dist/libc++_shared.so
  write_android_launcher dist/gproxy
  (cd dist && zip -9 "../$artifact.zip" gproxy gproxy.bin libc++_shared.so README.md)
  package_android_apk
else
  cp "$binary" dist/gproxy
  chmod 755 dist/gproxy
  (cd dist && zip -9 "../$artifact.zip" gproxy README.md)
fi

shasum -a 256 "$artifact.zip" > "$artifact.zip.sha256"
