#!/usr/bin/env bash
# The one place the Android toolchain is pinned. The release image
# (.gitlab/Dockerfile.android) and the F-Droid prebuild (fdroid-tools.sh) both
# install from here: the F-Droid APK must match the upstream one byte for byte,
# and a Rust, Go or Node bump on only one side breaks that (v4.1.11).
# Callers set RUSTUP_HOME and CARGO_HOME and provide rustup.
ANDROID_RUST_VERSION=1.99.1
ANDROID_GO_VERSION=1.27.2
ANDROID_GO_SHA256=ecbadb99091a3f46e31f5f934b068b1864eafa7995211b39eaddf76996045fe5
ANDROID_NODE_VERSION=24.22.1
ANDROID_NODE_SHA256=6c172a9ab6718f7b801fb5a7bd486af2ee5e32758086a04b84fcce2aa77095e6
ANDROID_GRADLE_VERSION=8.14.3
ANDROID_GRADLE_SHA256=bd71102213493060956ec229d946beee57158dbd89d0e62b91bca0fa2c5f3531
ANDROID_PNPM_VERSION=9.15.9

android_toolchain_download() {
  local url="$1" checksum="$2" file="$3"
  curl --fail --location --retry 5 --retry-all-errors "$url" -o "$file"
  printf '%s  %s\n' "$checksum" "$file" | sha256sum -c -
}

# Directories to put on PATH, in order, for a toolchain installed under $1.
# Versionless, so the release image's ENV PATH need not repeat the pins.
android_toolchain_path() {
  local dir="$1"
  printf '%s' "$dir/node/bin:$dir/pnpm/bin:$dir/go/bin:$dir/gradle/bin"
}

install_android_toolchain() {
  local dir="$1"
  mkdir -p "$dir"
  android_toolchain_download "https://nodejs.org/dist/v$ANDROID_NODE_VERSION/node-v$ANDROID_NODE_VERSION-linux-x64.tar.xz" \
    "$ANDROID_NODE_SHA256" "$dir/node.tar.xz"
  tar -xf "$dir/node.tar.xz" -C "$dir"
  mv "$dir/node-v$ANDROID_NODE_VERSION-linux-x64" "$dir/node"
  android_toolchain_download "https://go.dev/dl/go$ANDROID_GO_VERSION.linux-amd64.tar.gz" \
    "$ANDROID_GO_SHA256" "$dir/go.tar.gz"
  tar -xf "$dir/go.tar.gz" -C "$dir"
  android_toolchain_download "https://services.gradle.org/distributions/gradle-$ANDROID_GRADLE_VERSION-bin.zip" \
    "$ANDROID_GRADLE_SHA256" "$dir/gradle.zip"
  unzip -q -o "$dir/gradle.zip" -d "$dir"
  mv "$dir/gradle-$ANDROID_GRADLE_VERSION" "$dir/gradle"
  rm "$dir/node.tar.xz" "$dir/go.tar.gz" "$dir/gradle.zip"
  rustup toolchain install "$ANDROID_RUST_VERSION" \
    --profile minimal --target aarch64-linux-android,x86_64-linux-android
  rustup default "$ANDROID_RUST_VERSION"
  PATH="$dir/node/bin:$PATH" \
    npm install --prefix "$dir/pnpm" --global "pnpm@$ANDROID_PNPM_VERSION"
}
