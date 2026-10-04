#!/usr/bin/env bash
set -euo pipefail

# Records what a build actually resolved. Image tags float within a pinned
# line, so the tag alone does not identify a build six months later — this
# record does. One per artifact.

: "${ARTIFACT_NAME:?ARTIFACT_NAME is required}"
: "${GPROXY_VERSION:?GPROXY_VERSION is required}"

mkdir -p dist/release

version_of() { "$@" 2>/dev/null | head -1 || echo unknown; }

# Base images come from the container Dockerfile so the record cannot drift from the
# build. `docker image inspect` reports what the tag resolved to locally,
# which is the fact worth keeping.
images='[]'
if command -v docker >/dev/null 2>&1; then
  while read -r ref; do
    resolved="$(docker image inspect --format \
      '{{if .RepoDigests}}{{index .RepoDigests 0}}{{end}}' "$ref" 2>/dev/null || true)"
    images="$(jq --arg ref "$ref" --arg resolved "${resolved:-unresolved}" \
      '. + [{ref: $ref, resolved: $resolved}]' <<<"$images")"
  done < <(
    awk '/^FROM /  && $2 != "scratch" { print $2 }' deploy/container/Dockerfile
    if [[ "${BUILDER:-cargo}" = cargo-alpine || "${BUILDER:-cargo}" = tauri-alpine ]]; then
      awk '/^FROM / { print $2 }' deploy/container/Dockerfile.musl
    fi
    if [ "${BUILDER:-cargo}" = termux ]; then
      jq -r .image distribution/termux/toolchain.json
    fi
  )
fi

if [[ "${BUILDER:-cargo}" = cargo-alpine || "${BUILDER:-cargo}" = tauri-alpine ]] || [ "${BUILDER:-cargo}" = termux ]; then
  rustc_version="$(cat "target/${TARGET_TRIPLE:?}/release/rustc-version.txt")"
else
  rustc_version="$(version_of rustc --version)"
fi
if [ "${BUILDER:-cargo}" = termux ]; then
  commit="$(jq -r .commit "target/${TARGET_TRIPLE:?}/release/termux-toolchain.json")"
  node_version="$(jq -r .tools.node "target/${TARGET_TRIPLE:?}/release/termux-toolchain.json")"
  pnpm_version="$(jq -r .tools.pnpm "target/${TARGET_TRIPLE:?}/release/termux-toolchain.json")"
else
  commit="${GITHUB_SHA:-$(git rev-parse HEAD)}"
  node_version="$(version_of node --version)"
  pnpm_version="$(version_of pnpm --version)"
fi

jq -n \
  --arg version "$GPROXY_VERSION" \
  --arg commit "$commit" \
  --arg tag "${GITHUB_REF_NAME:-}" \
  --arg target "${TARGET_TRIPLE:-}" \
  --arg builder "${BUILDER:-cargo}" \
  --arg rustc "$rustc_version" \
  --arg node "$node_version" \
  --arg pnpm "$pnpm_version" \
  --arg upx_version "$(if [ "${BUILDER:-cargo}" = tauri-alpine ]; then cat "target/${TARGET_TRIPLE:?}/release/upx-version.txt"; elif [ "${UPX_ENABLED:-false}" = true ]; then version_of upx --version; else echo unused; fi)" \
  --argjson upx "${UPX_ENABLED:-false}" \
  --argjson images "$images" \
  '{version: $version, commit: $commit, tag: $tag, target: $target,
    builder: $builder,
    toolchain: {rustc: $rustc, node: $node, pnpm: $pnpm},
    compression: {upx: $upx, upx_version: $upx_version},
    images: $images}' \
  > "dist/release/${ARTIFACT_NAME}.provenance.json"

echo "wrote dist/release/${ARTIFACT_NAME}.provenance.json"

if [ "${BUILDER:-cargo}" = termux ]; then
  record="dist/release/$ARTIFACT_NAME.provenance.json"
  jq --slurpfile termux "target/${TARGET_TRIPLE:?}/release/termux-toolchain.json" \
    '.termux = $termux[0]' "$record" > "$record.tmp"
  mv "$record.tmp" "$record"
fi

if [[ "${TARGET_TRIPLE:-}" == *-linux-ohos ]]; then
  record="dist/release/$ARTIFACT_NAME.provenance.json"
  jq --arg image "${OHOS_TOOLCHAIN_IMAGE:-}" --arg builder "$BUILDER" \
    --slurpfile pins scripts/ohos/tauri-pins.json \
    '.ohos = {toolchain_image:$image,tauri:(if $builder=="tauri-ohos" then $pins[0] else null end),hap_signed:(if $builder=="tauri-ohos" then false else null end)}' \
    "$record" > "$record.tmp"
  mv "$record.tmp" "$record"
fi
