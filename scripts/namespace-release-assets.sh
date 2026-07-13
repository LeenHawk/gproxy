#!/usr/bin/env bash
set -euo pipefail

root="${1:?usage: namespace-release-assets.sh DIR SUFFIX}"
suffix="${2:?usage: namespace-release-assets.sh DIR SUFFIX}"
extensions=(zip apk msi dmg deb)

for extension in "${extensions[@]}"; do
  checksums=()
  while IFS= read -r -d '' checksum; do
    checksums+=("$checksum")
  done < <(find "$root" -type f -name "*.$extension.sha256" -print0)

  while IFS= read -r -d '' asset; do
    mv "$asset" "${asset%.$extension}-${suffix}.$extension"
  done < <(find "$root" -type f -name "*.$extension" -print0)

  for checksum in "${checksums[@]}"; do
    stem="${checksum%.$extension.sha256}"
    new_file="${stem}-${suffix}.$extension"
    sha="$(awk '{print $1}' "$checksum")"
    printf '%s  %s\n' "$sha" "$(basename "$new_file")" > "$new_file.sha256"
    rm "$checksum"
  done
done
