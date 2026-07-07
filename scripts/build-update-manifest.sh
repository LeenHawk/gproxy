#!/usr/bin/env bash
# Build + sign the self-update release manifest (manifest.json).
#
# The canonical signing payload MUST match, byte for byte, the Rust
# `Manifest::signing_payload()` in src/selfupdate/manifest.rs:
#
#   channel\n version\n notes_url(or "")\n min_compatible_data_version\n
#   then, per artifact in declared order: target_triple|url|sha256|size\n
#
# Artifacts point at the existing release `.zip` assets (the executable is
# extracted at apply time); `sha256` is the zip's sha, reused verbatim from each
# `<name>.zip.sha256` produced by the build job.
#
# Env:
#   UPDATE_SIGNING_PRIVATE_KEY_B64   base64 of the ed25519 PEM private key (required)
#   UPDATE_SIGNING_PUBLIC_KEY_B64    base64 of the 32-byte public key (required; sanity-checked)
#   TAG                              tag the assets live under, e.g. v2.0.6 or `staging` (required)
#   CHANNEL                          `releases` (default) or `staging`
#   VERSION                          override the manifest `version` (staging uses the commit sha;
#                                    releases default to `${TAG#v}`)
#   REPO                             owner/repo, e.g. LeenHawk/gproxy (required)
#   NOTES_URL                        release notes URL (optional)
#   ASSETS_DIR                       dir holding release-asset-<triple>/ subdirs (default: dl)
#   MIGRATIONS_FILE                  default: src/store/persistence/migrations.rs
#   OUT                              output path (default: manifest.json)
set -euo pipefail

: "${UPDATE_SIGNING_PRIVATE_KEY_B64:?missing UPDATE_SIGNING_PRIVATE_KEY_B64}"
: "${UPDATE_SIGNING_PUBLIC_KEY_B64:?missing UPDATE_SIGNING_PUBLIC_KEY_B64}"
: "${TAG:?missing TAG}"
: "${REPO:?missing REPO}"
NOTES_URL="${NOTES_URL:-}"
ASSETS_DIR="${ASSETS_DIR:-dl}"
MIGRATIONS_FILE="${MIGRATIONS_FILE:-src/store/persistence/migrations.rs}"
OUT="${OUT:-manifest.json}"

command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
command -v openssl >/dev/null || { echo "openssl is required" >&2; exit 1; }

channel="${CHANNEL:-releases}"
if [ "$channel" = "staging" ]; then
  # staging updates are decided by artifact sha256 (see src/selfupdate/version.rs),
  # so `version` is advisory only — carry the commit sha for a human-readable id.
  version="${VERSION:-staging}"
elif [ "$channel" = "releases" ]; then
  version="${VERSION:-${TAG#v}}"
  version="${version#v}"
  if ! [[ "$version" =~ ^[0-9]+[.][0-9]+[.][0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
    echo "release manifest version must be semver, got '$version'" >&2
    exit 1
  fi
else
  echo "unknown CHANNEL '$channel' (expected 'releases' or 'staging')" >&2
  exit 1
fi

# min_compatible_data_version is the oldest schema version this release can
# migrate/read from, not the newest schema it will migrate to. Source of truth:
# MIN_COMPATIBLE_DATA_VERSION in migrations.rs.
min_dv="$(grep -oP '^\s*pub const MIN_COMPATIBLE_DATA_VERSION:\s*i64\s*=\s*\K[0-9]+' "$MIGRATIONS_FILE")"
{ [ -n "$min_dv" ] && [ "$min_dv" -ge 1 ]; } || {
  echo "could not derive min_compatible_data_version from $MIGRATIONS_FILE" >&2; exit 1; }

# Self-updatable target triples — MUST match current_target_triple() in
# src/selfupdate/version.rs. This order is the manifest/payload declaration order.
triples=(
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  x86_64-apple-darwin
  aarch64-apple-darwin
  x86_64-pc-windows-msvc
  aarch64-pc-windows-msvc
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
payload="$work/payload"
printf '%s\n%s\n%s\n%s\n' "$channel" "$version" "$NOTES_URL" "$min_dv" > "$payload"

artifacts='[]'
for t in "${triples[@]}"; do
  dir="$ASSETS_DIR/release-asset-$t"
  zip="$(ls "$dir"/*.zip 2>/dev/null | head -1 || true)"
  shafile="$(ls "$dir"/*.zip.sha256 2>/dev/null | head -1 || true)"
  { [ -n "$zip" ] && [ -f "$zip" ]; } || { echo "missing .zip for $t in $dir" >&2; exit 1; }
  { [ -n "$shafile" ] && [ -f "$shafile" ]; } || { echo "missing .zip.sha256 for $t in $dir" >&2; exit 1; }
  asset="$(basename "$zip")"
  sha="$(awk '{print $1}' "$shafile")"
  size="$(stat -c%s "$zip")"
  url="https://github.com/$REPO/releases/download/$TAG/$asset"
  printf '%s|%s|%s|%s\n' "$t" "$url" "$sha" "$size" >> "$payload"
  artifacts="$(jq -c --arg t "$t" --arg u "$url" --arg s "$sha" --argjson z "$size" \
    '. + [{target_triple:$t, url:$u, sha256:$s, size:$z}]' <<<"$artifacts")"
done

# Decode private key; sanity-check it matches the configured public key (v1-style).
printf '%s' "$UPDATE_SIGNING_PRIVATE_KEY_B64" | base64 -d > "$work/priv.pem"
chmod 600 "$work/priv.pem"
derived="$(openssl pkey -in "$work/priv.pem" -pubout -outform DER | tail -c 32 | base64 -w0)"
[ "$derived" = "$UPDATE_SIGNING_PUBLIC_KEY_B64" ] || {
  echo "private key does not match configured public key (UPDATE_SIGNING_PUBLIC_KEY_B64)" >&2; exit 1; }

# Sign (pure Ed25519 over the canonical payload) → base64.
openssl pkeyutl -sign -rawin -inkey "$work/priv.pem" -in "$payload" -out "$work/sig.bin"
sig="$(base64 -w0 "$work/sig.bin")"

# Self-verify before emitting: no invalid manifest can leave this script.
openssl pkey -in "$work/priv.pem" -pubout -out "$work/pub.pem"
openssl pkeyutl -verify -rawin -pubin -inkey "$work/pub.pem" -sigfile "$work/sig.bin" -in "$payload" \
  >/dev/null || { echo "self-verify of manifest signature failed" >&2; exit 1; }

# Assemble manifest.json. notes_url "" → null (matches payload's unwrap_or("")).
jq -n --arg c "$channel" --arg v "$version" --arg n "$NOTES_URL" \
  --argjson mdv "$min_dv" --argjson arts "$artifacts" --arg sig "$sig" \
  '{channel:$c, version:$v, notes_url:(if $n=="" then null else $n end),
    min_compatible_data_version:$mdv, artifacts:$arts, signature:$sig}' > "$OUT"

echo "wrote $OUT (channel=$channel version=$version min_dv=$min_dv artifacts=${#triples[@]}):"
cat "$OUT"
