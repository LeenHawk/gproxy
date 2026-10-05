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
git fetch origin main
git merge-base --is-ancestor "$head" origin/main || {
  echo "version releases must come from main; merge and push main first" >&2
  exit 1
}
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  tagged="$(git rev-parse "$tag^{commit}")"
  if [ "$tagged" != "$head" ]; then
    echo "$tag already points at a different commit" >&2
    exit 1
  fi
fi

# Stable tags must include the latest dependency updates, reviewed and committed
# on main. Do not publish the old commit after an update changes the worktree.
if [[ "$version" != *-* ]]; then
  command -v pnpm >/dev/null || { echo "pnpm is required to refresh release dependencies" >&2; exit 1; }
  command -v cargo >/dev/null || { echo "cargo is required to refresh release dependencies" >&2; exit 1; }
  cargo upgrade --version >/dev/null 2>&1 || {
    echo "cargo-upgrade is required; install it with: cargo install cargo-edit --locked" >&2
    exit 1
  }
  pnpm --dir console update --latest
  pnpm --dir docs update --latest
  cargo upgrade --manifest-path Cargo.toml --incompatible allow --pinned allow
  cargo update
  if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "dependencies updated; validate, commit and push the changes to main, then rerun scripts/release.sh" >&2
    exit 1
  fi
fi

if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  git tag -a "$tag" -m "gproxy $tag"
fi

git push origin "refs/tags/$tag"
printf 'release workflow triggered for %s at %s\n' "$tag" "$head"
