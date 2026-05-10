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

repo_root() {
    git rev-parse --show-toplevel
}

is_submodule_path() {
    local path="$1"
    local root
    root="$(git -C "$path" rev-parse --show-toplevel 2>/dev/null || true)"

    [ -n "$root" ] && [ "$root" != "$(repo_root)" ]
}

commit_protocol_submodule_release_bump() {
    local protocol_dir="sdk/gproxy-protocol"

    if [ ! -d "$protocol_dir" ] || ! is_submodule_path "$protocol_dir"; then
        return
    fi

    local tracked_changes
    tracked_changes="$(git -C "$protocol_dir" status --porcelain --untracked-files=no)"
    if [ -z "$tracked_changes" ]; then
        return
    fi

    local changed_files
    changed_files="$(git -C "$protocol_dir" status --porcelain --untracked-files=no | cut -c4- | sort -u)"
    if [ "$changed_files" != "Cargo.toml" ]; then
        echo "Unexpected tracked changes in $protocol_dir:"
        git -C "$protocol_dir" status --short --untracked-files=no
        exit 1
    fi

    git -C "$protocol_dir" add Cargo.toml
    git -C "$protocol_dir" commit -m "Release v$VERSION"
    git -C "$protocol_dir" push
}

stage_release_files() {
    git add \
        Cargo.toml \
        Cargo.lock \
        apps/*/Cargo.toml \
        crates/*/Cargo.toml \
        "$RELEASE_NOTE_FILE"

    local manifest
    for manifest in sdk/*/Cargo.toml; do
        [ -e "$manifest" ] || continue

        local sdk_dir
        sdk_dir="$(dirname "$manifest")"
        if is_submodule_path "$sdk_dir"; then
            git add "$sdk_dir"
        else
            git add "$manifest"
        fi
    done
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

commit_protocol_submodule_release_bump
stage_release_files

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
