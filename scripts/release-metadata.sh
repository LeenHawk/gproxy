#!/usr/bin/env bash
set -euo pipefail

workspace_version() {
  awk '
    /^\[workspace\.package\]$/ { workspace = 1; next }
    /^\[/ && workspace { exit }
    workspace && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' Cargo.toml
}

version="$(workspace_version)"
if [ -z "$version" ]; then
  echo "workspace.package.version is missing" >&2
  exit 1
fi

case "${1:-}" in
  version)
    printf '%s\n' "$version"
    ;;
  verify-tag)
    tag="${2:?usage: $0 verify-tag TAG}"
    if [ "$tag" != "v$version" ]; then
      echo "tag $tag does not match workspace version v$version" >&2
      exit 1
    fi
    ;;
  *)
    echo "usage: $0 {version|verify-tag TAG}" >&2
    exit 1
    ;;
esac
