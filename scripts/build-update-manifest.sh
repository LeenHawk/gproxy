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
channel="${CHANNEL:-release}"
asset_base_url="${ASSET_BASE_URL:-https://github.com/$REPO/releases/download/$TAG}"
asset_base_url="${asset_base_url%/}"

case "$channel" in
  beta)
    if [ "$TAG" != staging ]; then scripts/release-metadata.sh verify-tag "$TAG"; fi
    ;;
  release)
    scripts/release-metadata.sh verify-tag "$TAG"
    if [ "$version" != "$(scripts/release-metadata.sh version)" ]; then
      echo "manifest version $version does not match workspace version" >&2
      exit 1
    fi
    ;;
  dev)
    test -n "$version" || { echo "dev manifest commit is required" >&2; exit 1; }
    ;;
  *)
    echo "unsupported update channel: $channel" >&2
    exit 1
    ;;
esac
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
command -v openssl >/dev/null || { echo "openssl is required" >&2; exit 1; }

schema_file="crates/gproxy/src/update/version.rs"
minimum="$(sed -n 's/^pub const DATA_VERSION: u32 = \([0-9][0-9]*\);$/\1/p' "$schema_file")"
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
  url="$asset_base_url/$artifact.zip"
  printf '%s|%s|%s|%s\n' "$target" "$url" "$sha" "$size" >> "$payload"
  artifacts="$(jq -c --arg t "$target" --arg u "$url" --arg s "$sha" --argjson z "$size" \
    '. + [{target_triple:$t,url:$u,sha256:$s,size:$z}]' <<<"$artifacts")"

  if [ "$os" = android ]; then
    # Only the Tauri Application is an APK; the CLI uses ZIP and Termux DEB.
    app_artifact="${artifact/gproxy-/gproxy-tauri-}"
    app_apk="$assets_dir/$app_artifact.apk"
    if [ -f "$app_apk" ]; then
      app_sha="$(awk '{print $1}' "$app_apk.sha256")"
      app_size="$(stat -c%s "$app_apk")"
      app_target="$target-tauri-apk"
      app_url="$asset_base_url/$app_artifact.apk"
      printf '%s|%s|%s|%s\n' "$app_target" "$app_url" "$app_sha" "$app_size" >> "$payload"
      artifacts="$(jq -c --arg t "$app_target" --arg u "$app_url" --arg s "$app_sha" \
        --argjson z "$app_size" '. + [{target_triple:$t,url:$u,sha256:$s,size:$z}]' \
        <<<"$artifacts")"
    fi
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
