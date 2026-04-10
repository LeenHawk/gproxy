#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
RELEASE_NOTE_FILE="RELEASE_NOTE.md"
TAG="v$VERSION"

if [ -z "$VERSION" ]; then
    echo "Usage: ./release.sh <version> (e.g., 1.0.0)"
    exit 1
fi

ensure_release_note_file() {
    if [ -f "$RELEASE_NOTE_FILE" ]; then
        return
    fi

    cat >"$RELEASE_NOTE_FILE" <<'NOTE'
# Release Notes
NOTE
}

append_release_note_template() {
    cat >>"$RELEASE_NOTE_FILE" <<NOTE

## v$VERSION

- TODO: summarize the changes in v$VERSION.
NOTE
}

extract_release_note_section() {
    awk -v section="## v$VERSION" '
        $0 == section { capture = 1; print; next }
        capture && /^## / { exit }
        capture { print }
    ' "$RELEASE_NOTE_FILE"
}

ensure_release_note_file
if ! grep -Fqx "## v$VERSION" "$RELEASE_NOTE_FILE"; then
    append_release_note_template
    echo "Added a release note template for v$VERSION in $RELEASE_NOTE_FILE."
    echo "Please update it before running release.sh again."
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found"
    exit 1
fi

if ! cargo set-version --help >/dev/null 2>&1; then
    echo "cargo set-version not found. Install with: cargo install cargo-edit"
    exit 1
fi

cargo update
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments
cargo set-version "$VERSION"
cargo check -p gproxy

git add \
    Cargo.toml \
    Cargo.lock \
    apps/*/Cargo.toml \
    crates/*/Cargo.toml \
    sdk/*/Cargo.toml \
    "$RELEASE_NOTE_FILE"

git commit -m "Release v$VERSION"
git push

tag_note_file="$(mktemp)"
{
    echo "v$VERSION"
    echo
    extract_release_note_section
} >"$tag_note_file"
git tag -a "$TAG" -F "$tag_note_file"
rm -f "$tag_note_file"

git push origin "$TAG"

if command -v gh >/dev/null 2>&1; then
    release_note_tmp="$(mktemp)"
    extract_release_note_section >"$release_note_tmp"
    if gh release view "$TAG" >/dev/null 2>&1; then
        gh release edit "$TAG" --title "$TAG" --notes-file "$release_note_tmp"
    else
        gh release create "$TAG" --title "$TAG" --notes-file "$release_note_tmp"
    fi
    rm -f "$release_note_tmp"
else
    echo "gh CLI not found, skipped GitHub Release body update."
fi
