#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

crates=(gproxy-protocol-macros gproxy-protocol gproxy-transform)
dry=0
if [ "${1:-}" = "--dry-run" ]; then
  dry=1
elif [ "$#" -ne 0 ]; then
  echo "usage: $0 [--dry-run]" >&2
  exit 2
fi

version="${VERSION:-$(scripts/release-metadata.sh version)}"
version="${version#v}"

if [ "$dry" -eq 0 ] && [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
  echo "CARGO_REGISTRY_TOKEN is not set" >&2
  exit 1
fi

index_path() {
  local name="$1"
  case "${#name}" in
    1) printf '1/%s\n' "$name" ;;
    2) printf '2/%s\n' "$name" ;;
    3) printf '3/%s/%s\n' "${name:0:1}" "$name" ;;
    *) printf '%s/%s/%s\n' "${name:0:2}" "${name:2:2}" "$name" ;;
  esac
}

published() {
  local name="$1"
  curl -fsSL --retry 5 --retry-all-errors \
    -H 'User-Agent: gproxy-release (https://github.com/LeenHawk/gproxy)' \
    "https://index.crates.io/$(index_path "$name")" 2>/dev/null \
    | grep -q "\"vers\":\"$version\""
}

pending=()
for name in "${crates[@]}"; do
  manifest="crates/$name/Cargo.toml"
  test -f "$manifest"
  if published "$name"; then
    echo "skip    $name $version"
  else
    echo "publish $name $version"
    pending+=(-p "$name")
  fi
done

if [ "${#pending[@]}" -eq 0 ]; then
  echo "nothing to publish"
  exit 0
fi

command=(cargo publish --locked "${pending[@]}")
if [ "$dry" -eq 1 ]; then
  command+=(--dry-run --allow-dirty)
fi
printf '+'
printf ' %q' "${command[@]}"
printf '\n'
if [ "$dry" -eq 1 ]; then
  package_target="$(mktemp -d)"
  trap 'rm -rf "$package_target"' EXIT
  CARGO_TARGET_DIR="$package_target" "${command[@]}"
else
  "${command[@]}"
fi
