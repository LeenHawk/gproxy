#!/usr/bin/env bash
set -euo pipefail

directory="${1:?asset directory is required}"
prefix="${2:?commit prefix is required}"
for file in "$directory"/*; do
  name="$(basename "$file")"
  if [ "$name" != manifest.json ] && [[ "$name" != "$prefix"* ]]; then
    mv "$file" "$directory/$prefix$name"
  fi
done
for file in "$directory"/*.sha256; do
  [ -f "$file" ] || continue
  checksum="$(awk '{print $1}' "$file")"
  printf '%s  %s\n' "$checksum" "$(basename "${file%.sha256}")" > "$file"
done
