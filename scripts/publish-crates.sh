#!/usr/bin/env bash
# Publish the reusable workspace libraries to crates.io.
#
# Called by .github/workflows/release.yml on a `vX.Y.Z` tag, so a release no
# longer needs a manual `cargo publish`. Safe to re-run: every crate/version
# already on the registry is skipped, and the script is a no-op when all of
# them are.
#
# The `gproxy` root crate is deliberately NOT published — it is the AGPL
# application, distributed as binaries/images, not as a library.
#
# Usage:
#   scripts/publish-crates.sh [--dry-run]
#
# Env:
#   VERSION               Version to publish (default: `version` in Cargo.toml).
#   CARGO_REGISTRY_TOKEN  crates.io token. Required unless --dry-run.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Dependency order: transform depends on protocol. `cargo publish` accepts the
# whole set in one invocation and handles ordering + index propagation itself,
# but the order here keeps the skip log readable.
CRATES=(gproxy-protocol gproxy-tokenize gproxy-transform)

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1

die() { echo "publish-crates.sh: $*" >&2; exit 1; }

crate_version() { grep -m1 '^version' "$1/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/'; }

# Sparse-index path for a crate name (see the crates.io index layout rules).
index_path() {
  local name="$1"
  case "${#name}" in
    1) echo "1/$name" ;;
    2) echo "2/$name" ;;
    3) echo "3/${name:0:1}/$name" ;;
    *) echo "${name:0:2}/${name:2:2}/$name" ;;
  esac
}

# 0 = this exact version is already on crates.io.
already_published() {
  local name="$1" version="$2" body
  body="$(curl -fsSL --retry 5 --retry-all-errors \
    -H 'User-Agent: gproxy-release (https://github.com/LeenHawk/gproxy)' \
    "https://index.crates.io/$(index_path "$name")" 2>/dev/null)" || return 1
  printf '%s\n' "$body" | grep -q "\"vers\":\"$version\""
}

VERSION="${VERSION:-$(crate_version .)}"
[ -n "$VERSION" ] || die "could not read version from Cargo.toml"
# Tag-shaped input (`v2.1.5`) is accepted so the workflow can pass the tag through.
VERSION="${VERSION#v}"

command -v cargo >/dev/null 2>&1 || die "cargo is required"
if [ "$DRY" = 0 ] && [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
  die "CARGO_REGISTRY_TOKEN is not set"
fi

pending=()
for name in "${CRATES[@]}"; do
  dir="crates/$name"
  [ -d "$dir" ] || die "missing crate directory: $dir"

  have="$(crate_version "$dir")"
  if [ "$have" != "$VERSION" ]; then
    die "$name is at $have but the release is $VERSION — run \`cargo set-version --workspace\`"
  fi

  if already_published "$name" "$VERSION"; then
    echo "skip    $name $VERSION (already on crates.io)"
  else
    echo "publish $name $VERSION"
    pending+=(-p "$name")
  fi
done

if [ "${#pending[@]}" -eq 0 ]; then
  echo "nothing to publish — every crate is already at $VERSION"
  exit 0
fi

# One invocation for the whole set: cargo resolves the intra-workspace path
# dependencies against the crates it is publishing in the same run, so
# gproxy-transform verifies without gproxy-protocol being on the index yet.
cmd=(cargo publish --locked "${pending[@]}")
[ "$DRY" = 1 ] && cmd+=(--dry-run)

echo "+ ${cmd[*]}"
"${cmd[@]}"
