#!/usr/bin/env bash
# Package the native iOS host after scripts/build-ios.sh. No Apple signing
# credentials or store upload is required for these CI artifacts.
set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
  echo 'iOS packaging requires macOS and Xcode 26.' >&2
  exit 1
fi

repo_dir=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_dir"
variant=${1:-device}
case "$variant" in
  device) sdk=iphoneos; rust_target=aarch64-apple-ios ;;
  simulator) sdk=iphonesimulator; rust_target=aarch64-apple-ios-sim ;;
  *) echo 'Expected device or simulator.' >&2; exit 1 ;;
esac

project="$repo_dir/crates/gproxy-host-ios/ios/Gproxy.xcodeproj"
test -d "$project"
test -f "$repo_dir/target/$rust_target/release/libgproxy_host_ios.a"
output="$repo_dir/dist/ios/$variant"
build_dir="$repo_dir/crates/gproxy-host-ios/ios/build/$variant"
log_dir="$repo_dir/dist/ios/logs"
mkdir -p "$output" "$build_dir" "$log_dir"

# Xcode's .app is built against the matching Rust device/simulator archive.
# Swift compilation and linking happen here, rather than only cargo check.
xcode_args=(
  -project "$project" -scheme Gproxy -configuration Release -sdk "$sdk"
  -derivedDataPath "$build_dir/DerivedData"
  ARCHS=arm64 ONLY_ACTIVE_ARCH=YES
  "GPROXY_RUST_TARGET=$rust_target"
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO 'CODE_SIGN_IDENTITY='
)

if [[ "$variant" == device ]]; then
  archive="$build_dir/Gproxy.xcarchive"
  xcodebuild "${xcode_args[@]}" -destination 'generic/platform=iOS' \
    -archivePath "$archive" archive 2>&1 | tee "$log_dir/$variant.log"
  app="$archive/Products/Applications/Gproxy.app"
  test -d "$app"
  # An unsigned archive cannot use Apple's signed exportArchive path. Keep
  # the standard IPA Payload layout for signing by a sideloading tool.
  work=$(mktemp -d "${TMPDIR:-/tmp}/gproxy-ios-package.XXXXXX")
  trap 'rm -rf "$work"' EXIT
  mkdir -p "$work/Payload"
  ditto "$app" "$work/Payload/Gproxy.app"
  package="$output/gproxy-ios-arm64-unsigned.ipa"
  ditto -c -k --keepParent "$work/Payload" "$package"
else
  xcodebuild "${xcode_args[@]}" -destination 'generic/platform=iOS Simulator' \
    build 2>&1 | tee "$log_dir/$variant.log"
  app="$build_dir/DerivedData/Build/Products/Release-iphonesimulator/Gproxy.app"
  test -d "$app"
  # ARM64 simulator executables need a local ad-hoc signature, not an Apple
  # developer certificate or provisioning profile.
  codesign --force --sign - "$app"
  codesign --verify "$app"
  package="$output/gproxy-ios-arm64-simulator.zip"
  ditto -c -k --keepParent "$app" "$package"
fi

executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Info.plist")
test -x "$app/$executable"
lipo -verify_arch arm64 "$app/$executable"
/usr/libexec/PlistBuddy -c 'Print :BGTaskSchedulerPermittedIdentifiers:0' "$app/Info.plist" \
  | grep -Fx 'com.leenhawk.gproxy.ios.proxy-session.*'
unzip -tq "$package"
(cd "$output" && shasum -a 256 "$(basename "$package")" > SHA256SUMS)
echo "Packaged $package"
