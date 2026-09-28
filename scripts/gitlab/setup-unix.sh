#!/usr/bin/env bash
# Sourced so each job keeps its tool paths. No signing material is installed here.
set -euo pipefail
tools_dir="$CI_PROJECT_DIR/.ci-tools"
mkdir -p "$tools_dir"
export CARGO_HOME="$CI_PROJECT_DIR/.cargo"
case "$(uname -m)" in x86_64) tool_arch=amd64; node_arch=x64 ;; *) tool_arch=arm64; node_arch=arm64 ;; esac
if [ "$(uname -s)" = Darwin ]; then
  tool_os=darwin
  export HOMEBREW_NO_AUTO_UPDATE=1
  brew install cmake pkgconf
  command -v jq >/dev/null || brew install jq
  export MACOSX_DEPLOYMENT_TARGET=11.0
  if [ "${TARGET_TRIPLE:-}" = x86_64-apple-darwin ]; then
    export MACOSX_DEPLOYMENT_TARGET=10.13
    arch -x86_64 /usr/bin/true || sudo softwareupdate --install-rosetta --agree-to-license
  fi
else
  tool_os=linux
  apt-get update
  apt-get install -y --no-install-recommends clang cmake libclang-dev pkg-config jq \
    zip unzip python3 openssl docker.io docker-cli docker-buildx libwebkit2gtk-4.1-dev libgtk-3-dev \
    libsoup-3.0-dev libdbus-1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
fi
curl -fsSL --retry 5 --retry-all-errors "https://go.dev/dl/go$GO_VERSION.$tool_os-$tool_arch.tar.gz" -o "$tools_dir/go.tar.gz"
tar -xzf "$tools_dir/go.tar.gz" -C "$tools_dir"
export GOROOT="$tools_dir/go"
export PATH="$tools_dir/go/bin:$PATH"
node_archive="node-v$NODE_VERSION-$tool_os-$node_arch.tar.gz"
curl -fsSL --retry 5 --retry-all-errors "https://nodejs.org/dist/v$NODE_VERSION/$node_archive" -o "$tools_dir/$node_archive"
curl -fsSL --retry 5 --retry-all-errors "https://nodejs.org/dist/v$NODE_VERSION/SHASUMS256.txt" -o "$tools_dir/node-shasums"
expected="$(awk -v file="$node_archive" '$2 == file {print $1}' "$tools_dir/node-shasums")"
if command -v sha256sum >/dev/null; then
  actual="$(sha256sum "$tools_dir/$node_archive" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$tools_dir/$node_archive" | awk '{print $1}')"
fi
test -n "$expected" && test "$actual" = "$expected"
tar -xzf "$tools_dir/$node_archive" -C "$tools_dir"
export PATH="$tools_dir/node-v$NODE_VERSION-$tool_os-$node_arch/bin:$PATH"
npm install -g pnpm@9.15.9
export PATH="$(npm prefix -g)/bin:$PATH"
if [ "$tool_os" = darwin ]; then
  # Bypass the image's asdf shims, which override CARGO_HOME with their own
  # preinstalled Rust directory and would bypass this project's cache.
  export RUSTUP_HOME="$tools_dir/rustup"
fi
if { [ "$tool_os" = darwin ] && [ ! -x "$CARGO_HOME/bin/rustup" ]; } || ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$tools_dir/rustup.sh"
  sh "$tools_dir/rustup.sh" -y --profile minimal --default-toolchain "$RUST_VERSION" --no-modify-path
fi
export PATH="$CARGO_HOME/bin:$HOME/.cargo/bin:$PATH"
rustup toolchain install "$RUST_VERSION" --profile minimal --component clippy,rustfmt
rustup default "$RUST_VERSION"
export RUSTUP_TOOLCHAIN="$RUST_VERSION"
rustup target add wasm32-unknown-unknown
if [ -n "${TARGET_TRIPLE:-}" ] && [[ "$TARGET_TRIPLE" != *-musl ]]; then rustup target add "$TARGET_TRIPLE"; fi
if [ "${TARGET_OS:-}" = android ]; then source scripts/gitlab/setup-android.sh; fi
go version
rustc --version
node --version
