#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: ./release.sh <version> (e.g., 0.1.0)"
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
cargo set-version "$VERSION"
cargo check -p gproxy

git add Cargo.toml Cargo.lock apps/gproxy/Cargo.toml crates/*/Cargo.toml

git commit -m "Release v$VERSION"
git push

git tag -a "v$VERSION" -m "Release v$VERSION"
git push origin "v$VERSION"
