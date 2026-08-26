#!/usr/bin/env bash
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
target_os="${TARGET_OS:?missing TARGET_OS}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
output_dir="${OUTPUT_DIR:-$PWD/dist/release}"
binary="target/$target/release/gproxy"

checksum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi
}

find_android_libcxx() {
  local root candidate
  for candidate in "${ANDROID_NDK_ROOT:-}" "${ANDROID_NDK_HOME:-}"; do
    if [ -n "$candidate" ] && [ -d "$candidate" ]; then
      root="$candidate"
      break
    fi
  done
  if [ -z "${root:-}" ]; then
    for candidate in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"; do
      if [ -n "$candidate" ] && [ -d "$candidate/ndk" ]; then
        root="$(find "$candidate/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
        break
      fi
    done
  fi
  if [ -z "${root:-}" ]; then
    echo "could not locate Android NDK" >&2
    exit 1
  fi
  local path
  path="$(find "$root/toolchains/llvm/prebuilt" -path "*/sysroot/usr/lib/$target/libc++_shared.so" -type f | sort | head -1)"
  if [ -z "$path" ]; then
    echo "could not locate libc++_shared.so for $target" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

write_android_launcher() {
  local path="$1"
  cp scripts/android/gproxy-launcher.sh "$path"
  chmod 755 "$path"
}

if [ ! -f "$binary" ]; then
  echo "missing release binary: $binary" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
package="$work/$artifact"
mkdir -p "$package" "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
install -m 0644 README.md LICENSE "$package/"

if [ "$target_os" = "android" ]; then
  install -m 0755 "$binary" "$package/gproxy.bin"
  install -m 0644 "$(find_android_libcxx)" "$package/libc++_shared.so"
  write_android_launcher "$package/gproxy"
else
  install -m 0755 "$binary" "$package/gproxy"
fi

archive="$output_dir/$artifact.zip"
rm -f "$archive" "$archive.sha256"
(cd "$package" && zip -9 -q -r "$archive" .)
(cd "$output_dir" && checksum "$artifact.zip" > "$artifact.zip.sha256")

case "$target_os" in
  linux) OUTPUT_DIR="$output_dir" scripts/package-linux-deb.sh ;;
  macos) OUTPUT_DIR="$output_dir" scripts/package-macos-dmg.sh ;;
esac
