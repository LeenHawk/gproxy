#!/usr/bin/env bash
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
binary="target/$target/release/gproxy"
version="$(awk '/^version = / { gsub(/"/, "", $3); print $3; exit }' Cargo.toml)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

app="$work/image/GPROXY.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 0755 "$binary" "$app/Contents/MacOS/gproxy"
install -m 0755 scripts/installers/macos/GPROXY "$app/Contents/MacOS/GPROXY"
sed "s/__VERSION__/$version/g" scripts/installers/macos/Info.plist.in \
  > "$app/Contents/Info.plist"
cp README.md "$work/image/README.md"
ln -s /Applications "$work/image/Applications"

codesign --force --deep --sign - "$app"
hdiutil create -quiet -volname GPROXY -srcfolder "$work/image" \
  -ov -format UDZO "$artifact.dmg"
shasum -a 256 "$artifact.dmg" > "$artifact.dmg.sha256"
