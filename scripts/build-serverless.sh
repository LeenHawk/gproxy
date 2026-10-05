#!/usr/bin/env bash
set -euo pipefail

target="${1:?target triple is required}"
case "$target:$(uname -m)" in
  x86_64-unknown-linux-musl:x86_64) platform=linux/amd64 ;;
  aarch64-unknown-linux-musl:aarch64) platform=linux/arm64 ;;
  *) echo "serverless builds require a matching native musl host" >&2; exit 1 ;;
esac
docker build --platform "$platform" -f deploy/container/Dockerfile.musl -t gproxy-musl-builder .
docker run --rm --platform "$platform" \
  --user "$(id -u):$(id -g)" \
  --volume "$PWD:/workspace" --workdir /workspace \
  --env CARGO_HOME=/workspace/target/.cache/cargo-musl \
  --env GOCACHE=/workspace/target/.cache/go-build \
  --env GOMODCACHE=/workspace/target/.cache/go-mod \
  --env 'RUSTFLAGS=-C target-feature=+crt-static' \
  --env SOURCE_DATE_EPOCH --env GPROXY_BUILD_HASH \
  gproxy-musl-builder sh -eu -c '
    native_target="$(rustc -vV | sed -n "s/^host: //p")"
    python3 scripts/reproducible-run.py cargo build --locked --release -p gproxy --bin gproxy-serverless \
      --target "$native_target" --no-default-features --features channels,postgres,embedded-console
    mkdir -p "target/$1/release"
    if [ "$native_target" != "$1" ]; then
      cp "target/$native_target/release/gproxy-serverless" "target/$1/release/gproxy-serverless"
    fi
    rustc --version > "target/$1/release/rustc-version.txt"
  ' sh "$target"
