#!/usr/bin/env bash
set -euo pipefail

: "${UPDATE_SIGNING_PRIVATE_KEY_B64:?missing UPDATE_SIGNING_PRIVATE_KEY_B64}"
: "${UPDATE_SIGNING_PUBLIC_KEY_B64:?missing UPDATE_SIGNING_PUBLIC_KEY_B64}"
: "${TAG:?missing TAG}"
: "${REPO:?missing REPO}"
assets_dir="${ASSETS_DIR:-dist/native}"
output="${OUT:-dist/release/manifest.json}"
notes_url="${NOTES_URL:-}"
version="${VERSION:-${TAG#v}}"
channel="${CHANNEL:-releases}"

case "$channel" in
  releases | dev)
    scripts/release-metadata.sh verify-tag "$TAG"
    if [ "$version" != "$(scripts/release-metadata.sh version)" ]; then
      echo "manifest version $version does not match workspace version" >&2
      exit 1
    fi
    ;;
  staging)
    test -n "$version" || { echo "staging manifest version is required" >&2; exit 1; }
    ;;
  *)
    echo "unsupported update channel: $channel" >&2
    exit 1
    ;;
esac
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
command -v openssl >/dev/null || { echo "openssl is required" >&2; exit 1; }

schema_file="crates/gproxy-store/src/schema/catalog.rs"
minimum="$(sed -n 's/^    Control = \([0-9][0-9]*\),$/\1/p' "$schema_file")"
if [ -z "$minimum" ]; then
  echo "could not derive minimum schema version" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
payload="$work/payload"
printf '%s\n%s\n%s\n%s\n' "$channel" "$version" "$notes_url" "$minimum" > "$payload"
artifacts='[]'

while IFS=$'\t' read -r target artifact os; do
  package="$assets_dir/$artifact.zip"
  sidecar="$package.sha256"
  if [ ! -f "$package" ] || [ ! -f "$sidecar" ]; then
    echo "missing native release archive for $target" >&2
    exit 1
  fi
  sha="$(awk '{print $1}' "$sidecar")"
  size="$(stat -c%s "$package")"
  url="https://github.com/$REPO/releases/download/$TAG/$artifact.zip"
  printf '%s|%s|%s|%s\n' "$target" "$url" "$sha" "$size" >> "$payload"
  artifacts="$(jq -c --arg t "$target" --arg u "$url" --arg s "$sha" --argjson z "$size" \
    '. + [{target_triple:$t,url:$u,sha256:$s,size:$z}]' <<<"$artifacts")"

  if [ "$os" = android ]; then
    apk="$assets_dir/$artifact.apk"
    apk_sidecar="$apk.sha256"
    if [ ! -f "$apk" ] || [ ! -f "$apk_sidecar" ]; then
      echo "missing signed Android APK for $target" >&2
      exit 1
    fi
    apk_sha="$(awk '{print $1}' "$apk_sidecar")"
    apk_size="$(stat -c%s "$apk")"
    apk_target="$target-apk"
    apk_url="https://github.com/$REPO/releases/download/$TAG/$artifact.apk"
    printf '%s|%s|%s|%s\n' "$apk_target" "$apk_url" "$apk_sha" "$apk_size" >> "$payload"
    artifacts="$(jq -c --arg t "$apk_target" --arg u "$apk_url" --arg s "$apk_sha" \
      --argjson z "$apk_size" '. + [{target_triple:$t,url:$u,sha256:$s,size:$z}]' \
      <<<"$artifacts")"
  fi
done < <(jq -r '.include[] | [.target,.artifact,.os] | @tsv' scripts/release-targets.json)

printf '%s' "$UPDATE_SIGNING_PRIVATE_KEY_B64" | base64 -d > "$work/private.pem"
chmod 600 "$work/private.pem"
derived="$(openssl pkey -in "$work/private.pem" -pubout -outform DER | tail -c 32 | base64 -w0)"
if [ "$derived" != "$UPDATE_SIGNING_PUBLIC_KEY_B64" ]; then
  echo "update signing private and public keys do not match" >&2
  exit 1
fi
openssl pkeyutl -sign -rawin -inkey "$work/private.pem" \
  -in "$payload" -out "$work/signature.bin"
signature="$(base64 -w0 "$work/signature.bin")"
openssl pkey -in "$work/private.pem" -pubout -out "$work/public.pem"
openssl pkeyutl -verify -rawin -pubin -inkey "$work/public.pem" \
  -sigfile "$work/signature.bin" -in "$payload" >/dev/null

mkdir -p "$(dirname "$output")"
jq -n --arg channel "$channel" --arg version "$version" --arg notes "$notes_url" \
  --argjson minimum "$minimum" --argjson artifacts "$artifacts" --arg signature "$signature" \
  '{channel:$channel,version:$version,notes_url:(if $notes=="" then null else $notes end),
    min_compatible_data_version:$minimum,artifacts:$artifacts,signature:$signature}' > "$output"
printf 'wrote signed update manifest with %s artifacts\n' "$(jq '.artifacts | length' "$output")"
