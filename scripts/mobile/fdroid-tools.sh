#!/usr/bin/env bash
# F-Droid prebuild: pinned FLOSS build tools, outside the scanned app checkout.
set -euo pipefail
rustup_source="${1:?path to F-Droid rustup srclib}"
tools_dir="$HOME/.local/share/gproxy-fdroid"
mkdir -p "$tools_dir"
download() {
  local url="$1" checksum="$2" file="$tools_dir/$3"
  curl --fail --location --retry 3 "$url" -o "$file"
  printf '%s  %s\n' "$checksum" "$file" | sha256sum -c -
}
download https://nodejs.org/dist/v24.21.0/node-v24.21.0-linux-x64.tar.xz \
  fd8e59d5a511510f6a298afb548f18c7d2b1be404d8b4a27d94fbe49f56cb2d6 node.tar.xz
tar -xf "$tools_dir/node.tar.xz" -C "$tools_dir"
download https://go.dev/dl/go1.27.1.linux-amd64.tar.gz \
  63d339f0da5ab53635a56f2490a7984dfe12dfcff22ad749f63edaf590168445 go.tar.gz
tar -xf "$tools_dir/go.tar.gz" -C "$tools_dir"
download https://services.gradle.org/distributions/gradle-8.14.3-bin.zip \
  bd71102213493060956ec229d946beee57158dbd89d0e62b91bca0fa2c5f3531 gradle.zip
unzip -q -o "$tools_dir/gradle.zip" -d "$tools_dir"
export RUSTUP_HOME="$tools_dir/rustup" CARGO_HOME="$tools_dir/cargo"
bash "$rustup_source/rustup-init.sh" -y --no-modify-path --default-toolchain 1.98.0 \
  --profile minimal --target aarch64-linux-android
export PATH="$tools_dir/node-v24.21.0-linux-x64/bin:$PATH"
npm install --prefix "$tools_dir/pnpm" --global pnpm@9.15.9
