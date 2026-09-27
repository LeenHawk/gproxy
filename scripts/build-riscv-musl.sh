#!/usr/bin/env bash
# Runs a native musl compiler on a native RISC-V host; no QEMU or cross-rs.
set -euo pipefail
test "$(uname -m)" = riscv64
target=riscv64gc-unknown-linux-musl
image=gproxy-riscv-musl-builder
docker build --platform linux/riscv64 \
  -f deploy/container/Dockerfile.riscv-musl -t "$image" .
docker run --rm --platform linux/riscv64 \
  --user "$(id -u):$(id -g)" \
  --volume "$PWD:/workspace" --workdir /workspace \
  --env CARGO_HOME=/workspace/target/.cache/cargo-musl \
  --env GOCACHE=/workspace/target/.cache/go-build \
  --env GOMODCACHE=/workspace/target/.cache/go-mod \
  --env CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER=cc \
  --env 'RUSTFLAGS=-C target-feature=+crt-static -C link-self-contained=no' \
  --env GPROXY_BUILD_VERSION --env GPROXY_BUILD_CHANNEL \
  --env GPROXY_BUILD_HASH --env GPROXY_UPDATE_PUBKEY \
  --env GPROXY_INSTALLATION_KIND \
  "$image" sh -eu -c '
    cargo build --locked --release --bin gproxy --target "$1"
    rustc --version > "target/$1/release/rustc-version.txt"
  ' sh "$target"
