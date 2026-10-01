#!/usr/bin/env bash
set -euo pipefail
source scripts/cnb/env.sh
export TAG="$RELEASE_TAG" REPO="$CNB_REPO_SLUG"
export CHANNEL="$GPROXY_BUILD_CHANNEL" VERSION="$GPROXY_BUILD_VERSION"
export NOTES_URL="https://cnb.cool/$REPO/-/releases/tag/$TAG"
export ASSET_BASE_URL="https://cnb.cool/$REPO/-/releases/download/$TAG"
export ASSETS_DIR=dist/release
if [ "$CHANNEL" = dev ]; then
  export VERSION="$CNB_COMMIT"
fi
node scripts/cnb/check-signing.mjs
for platform in github gitlab cnb; do
  case "$platform" in
    github) export ASSET_BASE_URL="https://github.com/$REPO/releases/download/$TAG" NOTES_URL="https://github.com/$REPO/releases/tag/$TAG" ;;
    gitlab) export ASSET_BASE_URL="$CI_PROJECT_URL/-/releases/$TAG/downloads" NOTES_URL="$CI_PROJECT_URL/-/releases/$TAG" ;;
    cnb) export ASSET_BASE_URL="https://cnb.cool/$REPO/-/releases/download/$TAG" NOTES_URL="https://cnb.cool/$REPO/-/releases/tag/$TAG" ;;
  esac
  export OUT="dist/manifests/$platform/manifest.json"
  scripts/build-update-manifest.sh
  if [ "$CHANNEL" = release ]; then
    CHANNEL=beta OUT="dist/manifests/$platform/beta/manifest.json" scripts/build-update-manifest.sh
  fi
done
