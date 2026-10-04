#!/usr/bin/env bash
# Build against Alpine's graphical libraries with a native musl toolchain.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
: "${TARGET_TRIPLE:?}"
case "$TARGET_TRIPLE:$(uname -m)" in
  x86_64-unknown-linux-musl:x86_64) platform=linux/amd64 ;;
  aarch64-unknown-linux-musl:aarch64) platform=linux/arm64 ;;
  *) echo "musl applications require a matching x86_64/aarch64 host" >&2; exit 1 ;;
esac
engine="${CONTAINER_ENGINE:-docker}"
image=gproxy-musl-application-builder
"$engine" build --platform "$platform" --build-arg APPLICATION=true \
  -f deploy/container/Dockerfile.musl -t "$image" .
"$engine" run --rm --platform "$platform" \
  --user "$(id -u):$(id -g)" \
  --volume "$root:/workspace" --workdir /workspace \
  --env CARGO_HOME=/workspace/target/.cache/cargo-musl \
  --env GOCACHE=/workspace/target/.cache/go-build \
  --env GOMODCACHE=/workspace/target/.cache/go-mod \
  --env 'RUSTFLAGS=-C target-feature=-crt-static' \
  --env TARGET_TRIPLE --env ARTIFACT_NAME \
  --env GPROXY_BUILD_VERSION --env GPROXY_BUILD_CHANNEL \
  --env GPROXY_BUILD_HASH --env GPROXY_UPDATE_PUBKEY \
  --env GPROXY_INSTALLATION_KIND --env GPROXY_BUILD_UPDATE_SOURCE \
  --env ALPINE_SIGNING_PRIVATE_KEY_B64 \
  "$image" bash -euo pipefail -c '
    export TAURI_CONFIG="$(node -e '\''console.log(JSON.stringify({version:process.env.GPROXY_BUILD_VERSION}))'\'')"
    native_target="$(rustc -vV | sed -n "s/^host: //p")"
    bash scripts/with-tauri-desktop-lib.sh cargo build --locked --release \
      -p gproxy-host-tauri --bin gproxy-desktop --target "$native_target" \
      --features tauri/custom-protocol
    mkdir -p "target/$TARGET_TRIPLE/release"
    binary="target/$TARGET_TRIPLE/release/gproxy-desktop"
    if [ "$native_target" != "$TARGET_TRIPLE" ]; then
      cp "target/$native_target/release/gproxy-desktop" "$binary"
    fi
    # Check dependencies before UPX hides the original ELF dynamic section.
    ldd "$binary"
    rustc --version > "target/$TARGET_TRIPLE/release/rustc-version.txt"
    upx --version | head -1 > "target/$TARGET_TRIPLE/release/upx-version.txt"
    bash scripts/package-alpine.sh application
  '
