#!/usr/bin/env bash
set -euo pipefail
source scripts/ohos/env.sh
: "${ARTIFACT_NAME:?}"
binary="target/$TARGET_TRIPLE/release/gproxy"
reader="$OHOS_NATIVE_HOME/llvm/bin/llvm-readelf"
"$reader" -h -l -d "$binary"
case "$ohos_arch" in
  aarch64) machine=AArch64 ;;
  x86_64) machine='Advanced Micro Devices X86-64' ;;
esac
"$reader" -h "$binary" | grep -F "$machine"
"$reader" -S "$binary" | grep -F .note.ohos.ident
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
install -m755 "$binary" "$work/gproxy"
install -m644 README.md LICENSE "$work/"
# The SDK's system libraries are import stubs; ship only its actual C++ runtime.
if "$reader" -d "$binary" | grep -Fq 'libc++_shared.so'; then
  runtime="$(find "$OHOS_NATIVE_HOME/llvm" -path "*/$ohos_arch-linux-ohos/libc++_shared.so" -type f -print -quit)"
  test -n "$runtime"
  install -m644 "$runtime" "$work/libc++_shared.so"
fi
cat > "$work/run-gproxy.sh" <<'LAUNCHER'
#!/bin/sh
set -eu
directory="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export LD_LIBRARY_PATH="$directory${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$directory/gproxy" "$@"
LAUNCHER
chmod 755 "$work/run-gproxy.sh"
cat > "$work/INSTALL.txt" <<'INSTALL'
GPROXY CLI for OpenHarmony / HarmonyOS NEXT

Extract all files into one writable directory on an OHOS device with shell
access. Run `sh ./run-gproxy.sh --help` or set LD_LIBRARY_PATH to this directory
before executing gproxy. The binary uses the native OHOS ABI, not Android.
No system service or boot registration is installed. This ZIP is a CLI build;
the separate HAP is the application. Device execution requires an OHOS device.
INSTALL
output="$PWD/dist/release"
mkdir -p "$output"
(cd "$work" && zip -9 -qr "$output/$ARTIFACT_NAME.zip" .)
(cd "$output" && sha256sum "$ARTIFACT_NAME.zip" > "$ARTIFACT_NAME.zip.sha256")
