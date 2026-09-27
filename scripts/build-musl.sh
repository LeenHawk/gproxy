#!/usr/bin/env bash
# Runs a native musl compiler on a matching host; no QEMU or cross-rs.
set -euo pipefail
target="${1:?target triple is required}"
case "$target:$(uname -m)" in
  x86_64-unknown-linux-musl:x86_64) platform=linux/amd64 ;;
  aarch64-unknown-linux-musl:aarch64) platform=linux/arm64 ;;
  riscv64gc-unknown-linux-musl:riscv64) platform=linux/riscv64 ;;
  *) echo "musl builds require a matching native host: $target on $(uname -m)" >&2; exit 1 ;;
esac
linker="CARGO_TARGET_${target^^}_LINKER"
linker="${linker//-/_}"
image=gproxy-musl-builder
docker build --platform "$platform" \
  -f deploy/container/Dockerfile.musl -t "$image" .
docker run --rm --platform "$platform" \
  --user "$(id -u):$(id -g)" \
  --volume "$PWD:/workspace" --workdir /workspace \
  --env CARGO_HOME=/workspace/target/.cache/cargo-musl \
  --env GOCACHE=/workspace/target/.cache/go-build \
  --env GOMODCACHE=/workspace/target/.cache/go-mod \
  --env "$linker=cc" \
  --env 'RUSTFLAGS=-C target-feature=+crt-static -C link-self-contained=no' \
  --env GPROXY_BUILD_VERSION --env GPROXY_BUILD_CHANNEL \
  --env GPROXY_BUILD_HASH --env GPROXY_UPDATE_PUBKEY \
  --env GPROXY_INSTALLATION_KIND \
  "$image" sh -eu -c '
    cargo build --locked --release --bin gproxy --target "$1"
    rustc --version > "target/$1/release/rustc-version.txt"
  ' sh "$target"
