#!/usr/bin/env bash
# Installs the release packer: LeenHawk/upx release builds of upstream upx/upx
# devel (scripts/install-android-upx.sh builds the same source). Upstream 5.3.0
# packs 64-bit Linux executables whose loader dies at startup when the escape
# hatch does not fit on the last page of .text, and its RISC-V AUIPC filter can
# corrupt data; devel fixes both. The fork only adds the release workflow and
# publishes no RISC-V host build; RISC-V targets are packed on x86_64 and
# AArch64 hosts.
set -euo pipefail

version="${UPX_VERSION:-gproxy-5.3.1-1}"
fork="https://github.com/LeenHawk/upx/releases/download/$version"
case "$version:$(uname -s):$(uname -m)" in
  gproxy-5.3.1-1:Linux:x86_64)
    url="$fork/upx-linux-x86_64"
    checksum=ff4af96c42abc02a14834d16fe84691d1386495749552ccfa183542ca354f952
    ;;
  gproxy-5.3.1-1:Linux:aarch64)
    url="$fork/upx-linux-aarch64"
    checksum=7d6ebc6393787e12e52b8902e653efe72bb0ba8fd1db59b0b6298bab5867a539
    ;;
  gproxy-5.3.1-1:MINGW*:x86_64 | gproxy-5.3.1-1:MSYS*:x86_64)
    url="$fork/upx-windows-x64.exe"
    checksum=c338fbd9be1f2f54cbfd7a86aa34c2f777291d6934d46f4a2a717411f585fb69
    ;;
  gproxy-5.3.1-1:MINGW*:aarch64 | gproxy-5.3.1-1:MSYS*:aarch64 | \
  gproxy-5.3.1-1:MINGW*:arm64 | gproxy-5.3.1-1:MSYS*:arm64)
    url="$fork/upx-windows-arm64.exe"
    checksum=8db9e72d7c545d011591442771cb12c43b9b284bf0622a7b2dbbeeca45b98f6e
    ;;
  *) echo "unsupported UPX version or build host" >&2; exit 1 ;;
esac

directory="${RUNNER_TEMP:?RUNNER_TEMP is required}/gproxy-upx"
mkdir -p "$directory/bin"
download="$directory/${url##*/}"
curl --proto '=https' --tlsv1.2 -fsSL --retry 5 --retry-all-errors "$url" -o "$download"
printf '%s  %s\n' "$checksum" "$download" | sha256sum -c -
executable=upx
case "$url" in *.exe) executable=upx.exe ;; esac
mv "$download" "$directory/bin/$executable"
chmod +x "$directory/bin/$executable"
printf '%s\n' "$directory/bin" >> "${GITHUB_PATH:?GITHUB_PATH is required}"
"$directory/bin/$executable" --version
