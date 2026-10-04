#!/usr/bin/env bash
# Repackage the already built musl CLI archives; no target execution or QEMU.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
: "${GPROXY_BUILD_VERSION:?}"
if [ "${1:-}" != --inside ]; then
  input="$(cd "${ASSETS_DIR:-dist/alpine-input}" && pwd)"
  mkdir -p "${OUTPUT_DIR:-dist/release}"
  output="$(cd "${OUTPUT_DIR:-dist/release}" && pwd)"
  image="$(awk '/^FROM / {print $2; exit}' deploy/container/Dockerfile.musl)"
  "${CONTAINER_ENGINE:-docker}" run --rm \
    --volume "$root:/workspace:ro" --workdir /workspace \
    --volume "$input:/archives:ro" --volume "$output:/output" \
    --env ASSETS_DIR=/archives --env OUTPUT_DIR=/output \
    --env GPROXY_BUILD_VERSION --env ALPINE_SIGNING_PRIVATE_KEY_B64 \
    "$image" sh -eu -c '
      apk add --no-cache bash fakeroot openssl jq unzip
      bash scripts/package-alpine-cli-release.sh --inside
    '
  exit 0
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
while IFS=$'\t' read -r target cli headless; do
  for mode in cli headless; do
    artifact="$cli"
    [ "$mode" != headless ] || artifact="$headless"
    (cd "$ASSETS_DIR" && sha256sum -c "$artifact.zip.sha256")
    mkdir -p "$work/$artifact"
    unzip -q "$ASSETS_DIR/$artifact.zip" gproxy -d "$work/$artifact"
    TARGET_TRIPLE="$target" ARTIFACT_NAME="$artifact" \
      ALPINE_BINARY="$work/$artifact/gproxy" bash scripts/package-alpine.sh "$mode"
  done
done < <(jq -r '.include[] | select(.target | endswith("-linux-musl")) | [.target,.artifact,.headless_artifact] | @tsv' scripts/release-targets.json)
