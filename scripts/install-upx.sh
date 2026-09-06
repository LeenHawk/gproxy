#!/usr/bin/env bash
set -euo pipefail

version=5.2.1
case "$(uname -s):$(uname -m)" in
  Linux:x86_64)
    platform=amd64_linux
    checksum=402162aad30af47e60dbd767fb2e64ca394ace9727ba1f40283641f1d1b91657
    extension=tar.xz
    executable=upx
    ;;
  Linux:aarch64)
    platform=arm64_linux
    checksum=a72d112c5970a904a31da0b9c84f919bc16b9a311787c12245508544a78c7d36
    extension=tar.xz
    executable=upx
    ;;
  MINGW*:x86_64 | MSYS*:x86_64)
    platform=win64
    checksum=eabc6792a347d45e945be7748423e7868fd01b0d2bcaa2f4b1031fd71ff69bda
    extension=zip
    executable=upx.exe
    ;;
  *) echo "unsupported UPX build host" >&2; exit 1 ;;
esac

directory="${RUNNER_TEMP:?RUNNER_TEMP is required}/gproxy-upx"
mkdir -p "$directory"
archive="$directory/upx.$extension"
curl --proto '=https' --tlsv1.2 -fsSL --retry 5 --retry-all-errors \
  "https://github.com/upx/upx/releases/download/v$version/upx-$version-$platform.$extension" \
  -o "$archive"
printf '%s  %s\n' "$checksum" "$archive" | sha256sum -c -
if [ "$extension" = zip ]; then
  python -m zipfile -e "$archive" "$directory"
else
  tar -xJf "$archive" -C "$directory"
fi
tool="$directory/upx-$version-$platform"
printf '%s\n' "$tool" >> "${GITHUB_PATH:?GITHUB_PATH is required}"
"$tool/$executable" --version
