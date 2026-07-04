#!/usr/bin/env bash
# Cut a gproxy release.
#
# Updates the workspace version, commits that release bump, tags it, and
# publishes a GitHub Release whose `release: published` event drives
# .github/workflows/release.yml — which builds the native binaries, edge wasm
# bundles, and multi-arch Docker images, attaches the assets, and force-refreshes
# the `deploy` branch. CI does all the building.
#
# Usage:
#   scripts/release.sh [-v VERSION] [-n NOTES_FILE] [--draft] [--dry-run] [-y]
#
#   -v VERSION     Release version (default: the `version` in Cargo.toml).
#                  Tag is `v$VERSION`, release title `gproxy v$VERSION`.
#   -n NOTES_FILE  Bilingual release notes (default: docs/release-notes/v$VERSION.md).
#                  Same shape as previous releases: a `## vX.Y.Z` heading, a `>`
#                  summary, then `### English` / `### 简体中文` with
#                  `#### Added` / `#### Fixed` / `#### Changed` sections.
#   --draft        Create a draft release. NOTE: a draft does NOT fire the
#                  release workflow — publish it later to trigger the build.
#   --dry-run      Print what would happen; create nothing.
#   -y             Skip the confirmation prompt.
#
# To fix the notes on an ALREADY-published release, don't re-run this — use:
#   gh release edit vX.Y.Z --notes-file docs/release-notes/vX.Y.Z.md
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION=""
NOTES=""
DRAFT=0
DRY=0
YES=0
TARGET="main"

while [ $# -gt 0 ]; do
  case "$1" in
    -v) VERSION="$2"; shift 2 ;;
    -n) NOTES="$2"; shift 2 ;;
    --draft)   DRAFT=1; shift ;;
    --dry-run) DRY=1; shift ;;
    -y)        YES=1; shift ;;
    -h|--help) sed -n '2,28p' "$0"; exit 0 ;;
    *) echo "release.sh: unknown arg: $1" >&2; exit 2 ;;
  esac
done

die() { echo "release.sh: $*" >&2; exit 1; }

read_cargo_version() {
  grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/'
}

stage_release_files() {
  git add \
    Cargo.toml \
    Cargo.lock \
    crates/gproxy-protocol/Cargo.toml \
    crates/gproxy-tokenize/Cargo.toml \
    crates/gproxy-transform/Cargo.toml \
    "$NOTES"
}

# --- Preconditions ----------------------------------------------------------
command -v gh  >/dev/null 2>&1 || die "the GitHub CLI (gh) is required"
command -v git >/dev/null 2>&1 || die "git is required"
command -v cargo >/dev/null 2>&1 || die "cargo is required"
[ -f Cargo.toml ] || die "must run from the crate root (no Cargo.toml at $ROOT)"
gh auth status >/dev/null 2>&1 || die "gh is not authenticated (run: gh auth login)"
if ! cargo set-version --help >/dev/null 2>&1; then
  die "cargo set-version not found. Install with: cargo install cargo-edit"
fi

# Version: explicit, else the crate version from Cargo.toml.
CURRENT_VERSION="$(read_cargo_version)"
[ -n "$CURRENT_VERSION" ] || die "could not read version from Cargo.toml"
if [ -z "$VERSION" ]; then
  VERSION="$CURRENT_VERSION"
fi
if ! [[ "$VERSION" =~ ^[0-9]+[.][0-9]+[.][0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  die "version must be semver, got: $VERSION"
fi
TAG="v$VERSION"

# Notes: explicit, else the per-version file under docs/release-notes/.
[ -n "$NOTES" ] || NOTES="docs/release-notes/$TAG.md"
[ -f "$NOTES" ] || die "release notes not found: $NOTES (create it, same format as past releases)"
[ -s "$NOTES" ] || die "release notes file is empty: $NOTES"

# The tag must not already exist (locally or on origin).
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null 2>&1 && die "tag $TAG already exists locally"
if git ls-remote --exit-code --tags origin "$TAG" >/dev/null 2>&1; then
  die "tag $TAG already exists on origin"
fi

# The release builds from $TARGET; warn if the working tree looks stale.
git fetch -q origin "$TARGET" 2>/dev/null || true
if ! git rev-parse --verify -q "origin/$TARGET" >/dev/null 2>&1; then
  die "origin/$TARGET not found"
fi
if ! git merge-base --is-ancestor "origin/$TARGET" HEAD; then
  die "HEAD does not contain origin/$TARGET; rebase/merge $TARGET before releasing"
fi

# --- Plan -------------------------------------------------------------------
VERSION_BUMP="no"
if [ "$CURRENT_VERSION" != "$VERSION" ]; then
  VERSION_BUMP="$CURRENT_VERSION -> $VERSION"
fi

echo "release.sh plan"
echo "  tag      : $TAG"
echo "  title    : gproxy $TAG"
echo "  target   : $TARGET ($(git rev-parse --short HEAD))"
echo "  notes    : $NOTES"
echo "  version  : $VERSION_BUMP"
echo "  draft    : $([ "$DRAFT" = 1 ] && echo yes || echo 'no (publishing fires release.yml)')"
echo

if [ "$DRY" = 1 ]; then
  echo "[dry-run] would run:"
  if [ "$CURRENT_VERSION" != "$VERSION" ]; then
    echo "  cargo set-version --workspace $VERSION"
    echo "  cargo update --workspace"
    echo "  cargo metadata --locked --no-deps --format-version 1 >/dev/null"
  fi
  echo "  git add Cargo.toml Cargo.lock crates/gproxy-*/Cargo.toml $NOTES"
  echo "  git commit -m \"Release $TAG\"  # if staged release files changed"
  echo "  git push origin HEAD:$TARGET    # if a release commit was created"
  echo "  git tag -a $TAG -F <release-note>"
  echo "  git push origin $TAG"
  echo "  gh release create $TAG --title \"gproxy $TAG\" \\"
  echo "    $([ "$DRAFT" = 1 ] && echo --draft || echo --latest) --notes-file $NOTES"
  exit 0
fi

if [ "$YES" != 1 ]; then
  printf "Publish %s now? [y/N] " "$TAG"
  read -r ans
  case "$ans" in y|Y|yes|YES) ;; *) echo "aborted."; exit 1 ;; esac
fi

# --- Version + tag -----------------------------------------------------------
if [ "$CURRENT_VERSION" != "$VERSION" ]; then
  cargo set-version --workspace "$VERSION"
  cargo update --workspace
  cargo metadata --locked --no-deps --format-version 1 >/dev/null
fi

stage_release_files
if ! git diff --cached --quiet; then
  git commit -m "Release $TAG"
  git push origin "HEAD:$TARGET"
fi

tag_note_file="$(mktemp)"
{
  echo "$TAG"
  echo
  cat "$NOTES"
} >"$tag_note_file"
git tag -a "$TAG" -F "$tag_note_file"
rm -f "$tag_note_file"
git push origin "$TAG"

# --- Publish ----------------------------------------------------------------
create_args=(release create "$TAG" --title "gproxy $TAG" --notes-file "$NOTES")
if [ "$DRAFT" = 1 ]; then
  create_args+=(--draft)
else
  create_args+=(--latest)
fi

gh "${create_args[@]}"

echo
if [ "$DRAFT" = 1 ]; then
  echo "Draft $TAG created (no build yet). Publish it to trigger release.yml:"
  echo "  gh release edit $TAG --draft=false"
else
  echo "Published $TAG. The release workflow is now building:"
  echo "  gh run list --workflow=release.yml --limit 3"
fi
