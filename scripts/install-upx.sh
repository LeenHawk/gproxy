#!/usr/bin/env bash
# Installs the release packer: gproxy's UPX fork, LeenHawk/upx
# (scripts/install-android-upx.sh builds the same revision from source and
# lists its fixes). Upstream 5.3.0 packs 64-bit Linux executables whose loader
# dies at startup when the escape hatch does not fit on the last page of
# .text, and its RISC-V AUIPC filter can corrupt data. The fork publishes no
# RISC-V host build; RISC-V targets are packed on x86_64 and AArch64 hosts.
set -euo pipefail

version="${UPX_VERSION:-gproxy-5.3.0-2}"
fork="https://github.com/LeenHawk/upx/releases/download/$version"
case "$version:$(uname -s):$(uname -m)" in
  gproxy-5.3.0-2:Linux:x86_64)
    url="$fork/upx-linux-x86_64"
    checksum=880b1f6b1fec3041d6969ad9be48af9dddcf8b9a80d5aaf4e0d186f10333ed8b
    ;;
  gproxy-5.3.0-2:Linux:aarch64)
    url="$fork/upx-linux-aarch64"
    checksum=0ff2e1c6b66b3d73cb808295177a4d1b4cd53904ec50a454ed52987a99b04026
    ;;
  gproxy-5.3.0-2:MINGW*:x86_64 | gproxy-5.3.0-2:MSYS*:x86_64)
    url="$fork/upx-windows-x64.exe"
    checksum=6a8b239520b2b91b75896abe76c67b3ce4e8b3fb319b010df906fbf86fbf06ec
    ;;
  gproxy-5.3.0-2:MINGW*:aarch64 | gproxy-5.3.0-2:MSYS*:aarch64 | \
  gproxy-5.3.0-2:MINGW*:arm64 | gproxy-5.3.0-2:MSYS*:arm64)
    url="$fork/upx-windows-arm64.exe"
    checksum=04a91297bf38e6ec796a70b19586202420ad20b5299d703f917691b7203d66fd
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
