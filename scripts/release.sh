#!/usr/bin/env bash
set -euo pipefail

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
version="$(scripts/release-metadata.sh version)"
tag="v$version"
scripts/release-metadata.sh verify-tag "$tag"
test -f "docs/release-notes/v$version.md" || { echo "release notes are required" >&2; exit 1; }

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "release requires a clean tracked worktree" >&2
  exit 1
fi

head="$(git rev-parse HEAD)"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  tagged="$(git rev-parse "$tag^{commit}")"
  if [ "$tagged" != "$head" ]; then
    echo "$tag already points at a different commit" >&2
    exit 1
  fi
else
  git tag -a "$tag" -m "gproxy $tag"
fi

git push origin "refs/tags/$tag"
printf 'release workflow triggered for %s at %s\n' "$tag" "$head"
