#!/usr/bin/env bash
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
output_dir="${OUTPUT_DIR:-$PWD/dist/release}"
binary="target/$target/release/gproxy"
version="$(scripts/release-metadata.sh version)"
bundle_version="${version%%-*}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
app="$work/image/GPROXY.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"

install -m 0755 "$binary" "$app/Contents/MacOS/gproxy-server"
install -m 0755 scripts/installers/macos/GPROXY "$app/Contents/MacOS/GPROXY"
sed "s/__BUNDLE_VERSION__/$bundle_version/g" scripts/installers/macos/Info.plist.in > "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"
cp README.md LICENSE "$work/image/"
ln -s /Applications "$work/image/Applications"

hdiutil create -quiet -volname GPROXY -srcfolder "$work/image" -ov -format UDZO "$output_dir/$artifact.dmg"
hdiutil verify -quiet "$output_dir/$artifact.dmg"
(cd "$output_dir" && shasum -a 256 "$artifact.dmg" > "$artifact.dmg.sha256")
