#!/usr/bin/env bash
# Copy the canonical edge runtime adapter into one self-contained platform tree.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$ROOT/deploy/edge-runtime.js"

case "${1:-}" in
  cloudflare) TARGET="$ROOT/deploy/cloudflare/src/_shared.js" ;;
  deno) TARGET="$ROOT/deploy/deno/_shared.js" ;;
  netlify) TARGET="$ROOT/deploy/netlify/edge-functions/_lib/_shared.js" ;;
  *)
    echo "usage: $0 {cloudflare|deno|netlify}" >&2
    exit 2
    ;;
esac

cp "$SOURCE" "$TARGET"
