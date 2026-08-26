android_sdk_root() {
  local candidate
  for candidate in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"; do
    if [ -n "$candidate" ] && [ -d "$candidate" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  echo "could not locate Android SDK" >&2
  exit 1
}

android_ndk_root() {
  local candidate sdk
  for candidate in "${ANDROID_NDK_ROOT:-}" "${ANDROID_NDK_HOME:-}"; do
    if [ -n "$candidate" ] && [ -d "$candidate" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  sdk="$(android_sdk_root)"
  candidate="$(find "$sdk/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
  if [ -z "$candidate" ]; then
    echo "could not locate Android NDK" >&2
    exit 1
  fi
  printf '%s\n' "$candidate"
}

android_platform_jar() {
  local sdk="$1" path
  path="$(find "$sdk/platforms" -maxdepth 2 -name android.jar -type f | sort -V | tail -1)"
  if [ -z "$path" ]; then
    echo "could not locate android.jar" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

android_build_tool() {
  local sdk="$1" name="$2" path
  path="$(find "$sdk/build-tools" -maxdepth 2 -name "$name" -type f | sort -V | tail -1)"
  if [ -z "$path" ]; then
    echo "could not locate Android build tool $name" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

android_d8() {
  local sdk="$1" path
  path="$(find "$sdk" -name d8 -type f | sort -V | tail -1)"
  if [ -z "$path" ]; then
    echo "could not locate Android build tool d8" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}

android_abi() {
  case "$1" in
    aarch64-linux-android) printf '%s\n' arm64-v8a ;;
    x86_64-linux-android) printf '%s\n' x86_64 ;;
    *) echo "unsupported Android target: $1" >&2; exit 1 ;;
  esac
}

android_libcxx() {
  local target="$1" root path
  root="$(android_ndk_root)"
  path="$(find "$root/toolchains/llvm/prebuilt" \
    -path "*/sysroot/usr/lib/$target/libc++_shared.so" -type f | sort | head -1)"
  if [ -z "$path" ]; then
    echo "could not locate libc++_shared.so for $target" >&2
    exit 1
  fi
  printf '%s\n' "$path"
}
