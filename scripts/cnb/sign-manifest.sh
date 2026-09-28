#!/usr/bin/env bash
set -euo pipefail
source scripts/cnb/env.sh
export TAG="$RELEASE_TAG" REPO="$CNB_REPO_SLUG"
export CHANNEL="$GPROXY_BUILD_CHANNEL" VERSION="$GPROXY_BUILD_VERSION"
export NOTES_URL="https://cnb.cool/$REPO/-/releases/tag/$TAG"
export ASSET_BASE_URL="https://cnb.cool/$REPO/-/releases/download/$TAG"
export ASSETS_DIR=dist/release OUT=dist/release/manifest.json
export ASSET_PREFIX=
if [ "$CHANNEL" = dev ]; then
  export VERSION="$CNB_COMMIT" ASSET_PREFIX="$CNB_COMMIT-"
  scripts/namespace-nightly-assets.sh dist/release "$ASSET_PREFIX"
fi
node scripts/cnb/check-signing.mjs
scripts/build-update-manifest.sh
