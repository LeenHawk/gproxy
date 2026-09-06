#!/usr/bin/env bash
set -euo pipefail

# 5.x packs the x86_64 Linux executable into one that traps at startup;
# 4.2.4 is the last release whose output runs.
version=4.2.4
case "$(uname -s):$(uname -m)" in
  Linux:x86_64)
    platform=amd64_linux
    checksum=75cab4e57ab72fb4585ee45ff36388d280c7afd72aa03e8d4b9c3cbddb474193
    extension=tar.xz
    executable=upx
    ;;
  Linux:aarch64)
    platform=arm64_linux
    checksum=6bfeae6714e34a82e63245289888719c41fd6af29f749a44ae3d3d166ba6a1c9
    extension=tar.xz
    executable=upx
    ;;
  MINGW*:x86_64 | MSYS*:x86_64)
    platform=win64
    checksum=22e9ef20e4c72aad85e32c71cbc9c086436c179456382aa75c0c24868456a671
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
