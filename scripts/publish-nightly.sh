#!/usr/bin/env bash
set -euo pipefail

latest="$(git ls-remote origin refs/heads/dev | cut -f1)"
if [ "$latest" != "$GITHUB_SHA" ]; then
  echo "Skipping stale nightly publication: dev has advanced."
  exit 0
fi
test "$(jq -r .channel dist/publish/manifest.json)" = dev
test "$(jq -r .version dist/publish/manifest.json)" = "$GITHUB_SHA"
previous="$(mktemp -d)"
trap 'rm -rf "$previous"' EXIT
git tag -f nightly "$GITHUB_SHA"
git push -f origin refs/tags/nightly
notes="docs/release-notes/v$VERSION.md"
test -f "$notes"
if gh release view nightly >/dev/null 2>&1; then
  gh release edit nightly --target "$GITHUB_SHA" --title "gproxy dev" --notes-file "$notes" --prerelease --latest=false
else
  gh release create nightly --verify-tag --title "gproxy dev" --notes-file "$notes" --prerelease --latest=false
fi
mapfile -d '' files < <(find dist/publish -maxdepth 1 -type f ! -name manifest.json ! -name "*.sha256" ! -name "*.provenance.json" -print0 | sort -z)
gh release upload nightly "${files[@]}" --clobber
gh release upload nightly dist/publish/manifest.json --clobber
printf '%s\n' manifest.json > "$previous/keep"
find dist/publish -maxdepth 1 -type f ! -name "*.sha256" ! -name "*.provenance.json" -printf '%f\n' >> "$previous/keep"
gh api "repos/$GITHUB_REPOSITORY/releases/tags/nightly" --jq '.assets[].name' | while IFS= read -r name; do
  if ! grep -Fxq "$name" "$previous/keep"; then
    gh release delete-asset nightly "$name" --yes
  fi
done
