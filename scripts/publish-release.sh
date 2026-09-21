#!/usr/bin/env bash
set -euo pipefail

# Checksums feed the signed manifest; build records are hosted as attestations.
find dist/publish -maxdepth 1 -type f \( -name '*.sha256' -o -name '*.provenance.json' \) -delete

# Three channels. `dev` is every push to the development branch: no version
# tag exists, so the build is published under a floating pointer with its
# assets namespaced by commit. `beta` and `release` are both version tags and
# share the release-creation path below; only `beta` additionally moves a
# floating pointer, so a beta subscriber always resolves the newest one.
if [ "$PUBLISH_CHANNEL" = dev ]; then
  scripts/publish-nightly.sh
  exit 0
fi

mapfile -d '' files < <(find dist/publish -maxdepth 1 -type f ! -name manifest.json -print0 | sort -z)
test "${#files[@]}" -gt 0
release_flags=(--prerelease=false --latest=true)
if [[ "$VERSION" == *-* ]]; then release_flags=(--prerelease --latest=false); fi
notes="docs/release-notes/v$VERSION.md"
test -f "$notes"
if gh release view "$RELEASE_TAG" >/dev/null 2>&1; then
  gh release upload "$RELEASE_TAG" "${files[@]}" --clobber
  gh release upload "$RELEASE_TAG" dist/publish/manifest.json --clobber
  gh release edit "$RELEASE_TAG" --target "$GITHUB_SHA" --draft=false --notes-file "$notes" "${release_flags[@]}"
else
  gh release create "$RELEASE_TAG" "${files[@]}" dist/publish/manifest.json \
    --verify-tag --title "gproxy $RELEASE_TAG" --notes-file "$notes" "${release_flags[@]}"
fi
# A beta is a pre-release tag, and the channel keeps one floating pointer at
# the newest of them so a subscriber resolves it without knowing its version.
# The guard below refuses to move that pointer backwards when an older tag is
# (re)published after a newer one.
if [ "$PUBLISH_CHANNEL" = beta ]; then
  latest_beta_version="$(gh release list --limit 100 \
    --json tagName,isDraft,isPrerelease \
    --jq '.[] | select(.isPrerelease and (.isDraft | not)) | .tagName' \
    | sed -n 's/^v\(4\..*\)$/\1/p' | sort -V | tail -1)"
  if [ -n "$latest_beta_version" ] && [ "$latest_beta_version" != "$VERSION" ]; then
    echo "Skipping stale beta pointer update for $VERSION; latest prerelease is $latest_beta_version."
    exit 0
  fi
  git tag -f beta "$GITHUB_SHA"
  git push -f origin refs/tags/beta
  notes="Latest signed manifest for the GPROXY beta channel ($RELEASE_TAG)."
  if gh release view beta >/dev/null 2>&1; then
    gh release edit beta --target "$GITHUB_SHA" --title "gproxy beta" \
      --notes "$notes" --prerelease
  else
    gh release create beta --verify-tag --title "gproxy beta" \
      --notes "$notes" --prerelease
  fi
  gh release upload beta dist/publish/manifest.json --clobber
fi
