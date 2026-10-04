#!/usr/bin/env bash
# Run on macOS with Xcode 26 selected. Builds the native library and generates
# the Xcode project; signing and installation are performed in Xcode.
set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
  echo 'iOS builds require macOS and Xcode 26.' >&2
  exit 1
fi

repo_dir=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_dir"
ios_target=${1:-aarch64-apple-ios}
case "$ios_target" in
  aarch64-apple-ios|aarch64-apple-ios-sim) ;;
  *) echo 'Expected aarch64-apple-ios or aarch64-apple-ios-sim.' >&2; exit 1 ;;
esac

for tool in cargo rustup xcodegen cmake go; do
  command -v "$tool" >/dev/null || { echo "Missing tool: $tool" >&2; exit 1; }
done
ios_sdk=iphoneos
[[ "$ios_target" != aarch64-apple-ios-sim ]] || ios_sdk=iphonesimulator
sdk_version=$(xcrun --sdk "$ios_sdk" --show-sdk-version)
if [[ ${sdk_version%%.*} -lt 26 ]]; then
  echo "iOS SDK >=26 is required; selected SDK is $sdk_version." >&2
  exit 1
fi
export SDKROOT
SDKROOT=$(xcrun --sdk "$ios_sdk" --show-sdk-path)
export IPHONEOS_DEPLOYMENT_TARGET=26.0
rustup target add "$ios_target"
if [[ ${GPROXY_IOS_SKIP_FRONTEND:-0} == 1 ]]; then
  test -f crates/gproxy-host-axum/assets/web/index.html
else
  for tool in pnpm node; do
    command -v "$tool" >/dev/null || { echo "Missing tool: $tool" >&2; exit 1; }
  done
  node -e 'const [a,b]=process.versions.node.split(".").map(Number);if(a<22||(a===22&&b<12)){throw Error("Node >=22.12 is required")}'
  pnpm -C console install --frozen-lockfile
  pnpm -C console build
fi
cargo build --locked --release -p gproxy-host-ios --target "$ios_target"
xcodegen generate --spec crates/gproxy-host-ios/ios/project.yml
echo 'Open crates/gproxy-host-ios/ios/Gproxy.xcodeproj, select your signing team, and run on iOS 26.'
