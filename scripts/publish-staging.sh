#!/usr/bin/env bash
set -euo pipefail

latest="$(git ls-remote origin refs/heads/main | cut -f1)"
if [ "$latest" != "$GITHUB_SHA" ]; then
  echo "Skipping stale staging publication: main has advanced."
  exit 0
fi
test "$(jq -r .channel dist/publish/manifest.json)" = staging
test "$(jq -r .version dist/publish/manifest.json)" = "$GITHUB_SHA"
for file in dist/publish/*; do
  if [ "$(basename "$file")" != manifest.json ]; then
    mv "$file" "$(dirname "$file")/$GITHUB_SHA-$(basename "$file")"
  fi
done
previous="$(mktemp -d)"
trap 'rm -rf "$previous"' EXIT
gh release download staging --pattern manifest.json --dir "$previous" >/dev/null 2>&1 || true
git tag -f staging "$GITHUB_SHA"
git push -f origin refs/tags/staging
notes="docs/release-notes/v$VERSION.md"
test -f "$notes"
if gh release view staging >/dev/null 2>&1; then
  gh release edit staging --target "$GITHUB_SHA" --title "gproxy v3 staging" --notes-file "$notes" --prerelease --latest=false
else
  gh release create staging --verify-tag --title "gproxy v3 staging" --notes-file "$notes" --prerelease --latest=false
fi
mapfile -d '' files < <(find dist/publish -maxdepth 1 -type f ! -name manifest.json -print0 | sort -z)
gh release upload staging "${files[@]}" --clobber
gh release upload staging dist/publish/manifest.json --clobber
printf '%s\n' manifest.json > "$previous/keep"
find dist/publish -maxdepth 1 -type f -printf '%f\n' >> "$previous/keep"
if [ -f "$previous/manifest.json" ]; then
  jq -r '.artifacts[].url | split("/")[-1]' "$previous/manifest.json" >> "$previous/keep"
fi
gh api "repos/$GITHUB_REPOSITORY/releases/tags/staging" --jq '.assets[].name' | while IFS= read -r name; do
  if [[ "$name" =~ ^[0-9a-f]{40}- ]] && ! grep -Fxq "$name" "$previous/keep"; then
    gh release delete-asset staging "$name" --yes
  fi
done
