#!/usr/bin/env bash
set -euo pipefail

if [ "$PUBLISH_CHANNEL" = staging ]; then
  scripts/publish-staging.sh
  exit 0
fi

mapfile -d '' files < <(find dist/publish -maxdepth 1 -type f -print0 | sort -z)
test "${#files[@]}" -gt 0
release_flags=()
if [[ "$VERSION" == *-* ]]; then release_flags+=(--prerelease); fi
if gh release view "$RELEASE_TAG" >/dev/null 2>&1; then
  gh release upload "$RELEASE_TAG" "${files[@]}" --clobber
  if [[ "$VERSION" == *-* ]]; then gh release edit "$RELEASE_TAG" --prerelease; fi
else
  gh release create "$RELEASE_TAG" "${files[@]}" \
    --verify-tag --title "gproxy $RELEASE_TAG" --generate-notes "${release_flags[@]}"
fi
if [ "$PUBLISH_CHANNEL" = dev ]; then
  latest_dev_version="$(gh release list --limit 100 \
    --json tagName,isDraft,isPrerelease \
    --jq '.[] | select(.isPrerelease and (.isDraft | not)) | .tagName' \
    | sed -n 's/^v\(3\..*\)$/\1/p' | sort -V | tail -1)"
  if [ -n "$latest_dev_version" ] && [ "$latest_dev_version" != "$VERSION" ]; then
    echo "Skipping stale dev pointer update for $VERSION; latest prerelease is $latest_dev_version."
    exit 0
  fi
  git tag -f dev "$GITHUB_SHA"
  git push -f origin refs/tags/dev
  notes="Latest signed manifest for the GPROXY v3 alpha channel ($RELEASE_TAG)."
  if gh release view dev >/dev/null 2>&1; then
    gh release edit dev --target "$GITHUB_SHA" --title "gproxy v3 dev" \
      --notes "$notes" --prerelease
  else
    gh release create dev --verify-tag --title "gproxy v3 dev" \
      --notes "$notes" --prerelease
  fi
  gh release upload dev dist/publish/manifest.json --clobber
fi
