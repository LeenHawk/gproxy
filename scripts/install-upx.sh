#!/usr/bin/env bash
# Installs the release packer. The default is gproxy's UPX fork, LeenHawk/upx
# (scripts/install-android-upx.sh builds the same revision from source): upx
# 5.2.1 and 5.3.0 pack AArch64 Linux executables whose loader dies with SIGTRAP
# when the escape hatch does not fit on the last page of .text.
set -euo pipefail

version="${UPX_VERSION:-gproxy-5.3.0-1}"
fork="https://github.com/LeenHawk/upx/releases/download/$version"
case "$version:$(uname -s):$(uname -m)" in
  4.2.4:Linux:x86_64)
    url=https://github.com/upx/upx/releases/download/v4.2.4/upx-4.2.4-amd64_linux.tar.xz
    checksum=75cab4e57ab72fb4585ee45ff36388d280c7afd72aa03e8d4b9c3cbddb474193
    member=upx-4.2.4-amd64_linux/upx
    ;;
  gproxy-5.3.0-1:Linux:x86_64)
    url="$fork/upx-linux-x86_64"
    checksum=96d0cbf041e2ed80e6c05344f5065518466dd625f79a6757ecb283d763a0f43b
    ;;
  gproxy-5.3.0-1:Linux:aarch64)
    url="$fork/upx-linux-aarch64"
    checksum=79b16b6a9bfea491b720a599c9564ff722bf897d7924d05f545cdeeb598d8c14
    ;;
  gproxy-5.3.0-1:Linux:riscv64)
    # The fork publishes no RISC-V build; this host packs only RISC-V, which
    # the AArch64 loader bug does not affect.
    url=https://github.com/upx/upx/releases/download/v5.2.1/upx-5.2.1-riscv64_linux.tar.xz
    checksum=a8663b4cb6523104a8a807362d0da6f5d6d55f6083152f78fa27fa9c05443c17
    member=upx-5.2.1-riscv64_linux/upx
    ;;
  gproxy-5.3.0-1:MINGW*:x86_64 | gproxy-5.3.0-1:MSYS*:x86_64)
    url="$fork/upx-windows-x64.exe"
    checksum=97898b22803b927ec9c95f6cf33a78242fee3acfefbd8fa4eb59f833142f1505
    ;;
  gproxy-5.3.0-1:MINGW*:aarch64 | gproxy-5.3.0-1:MSYS*:aarch64 | \
  gproxy-5.3.0-1:MINGW*:arm64 | gproxy-5.3.0-1:MSYS*:arm64)
    url="$fork/upx-windows-arm64.exe"
    checksum=593e2f0f3cadfc00b48d8403352901ff7f140b0a0417096d7855a70f7ec3fdc6
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
if [ -n "${member:-}" ]; then
  tar -xJf "$download" -C "$directory" "$member"
  mv "$directory/$member" "$directory/bin/$executable"
else
  mv "$download" "$directory/bin/$executable"
fi
chmod +x "$directory/bin/$executable"
printf '%s\n' "$directory/bin" >> "${GITHUB_PATH:?GITHUB_PATH is required}"
"$directory/bin/$executable" --version
