#!/usr/bin/env bash
# Package a Unix release build into the zip shape published by release.yml.
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
target_os="${TARGET_OS:?missing TARGET_OS}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
binary="target/$target/release/gproxy"

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
else
  cp "$binary" dist/gproxy
  chmod 755 dist/gproxy
  (cd dist && zip -9 "../$artifact.zip" gproxy README.md)
fi

shasum -a 256 "$artifact.zip" > "$artifact.zip.sha256"
