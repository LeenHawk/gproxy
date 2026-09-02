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
  done < <(awk '/^FROM /  && $2 != "scratch" { print $2 }' deploy/container/Dockerfile)
fi

jq -n \
  --arg version "$GPROXY_VERSION" \
  --arg commit "${GITHUB_SHA:-$(git rev-parse HEAD)}" \
  --arg tag "${GITHUB_REF_NAME:-}" \
  --arg target "${TARGET_TRIPLE:-}" \
  --arg builder "${BUILDER:-cargo}" \
  --arg rustc "$(version_of rustc --version)" \
  --arg node "$(version_of node --version)" \
  --arg pnpm "$(version_of pnpm --version)" \
  --argjson images "$images" \
  '{version: $version, commit: $commit, tag: $tag, target: $target,
    builder: $builder,
    toolchain: {rustc: $rustc, node: $node, pnpm: $pnpm},
    images: $images}' \
  > "dist/release/${ARTIFACT_NAME}.provenance.json"

echo "wrote dist/release/${ARTIFACT_NAME}.provenance.json"
