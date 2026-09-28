#!/usr/bin/env bash
set -euo pipefail
source scripts/gitlab/env.sh
: "${GH_TOKEN:?}" "${GITLAB_RELEASE_TOKEN:?}" "${CNB_TOKEN:?}"
test "$GPROXY_UPDATE_PUBKEY" = "$UPDATE_SIGNING_PUBLIC_KEY_B64"
if [ "$GPROXY_BUILD_CHANNEL" = dev ]; then
  latest="$(git ls-remote "https://github.com/$GITHUB_REPOSITORY.git" refs/heads/dev | cut -f1)"
  if [ "$latest" != "$CI_COMMIT_SHA" ]; then
    echo "Skipping stale nightly: GitHub dev has advanced."
    exit 0
  fi
fi
cp dist/msix/* dist/release/
cp dist/dmg/* dist/release/
VERIFY_ONLY=true python3 scripts/gitlab/publish.py
export TAG="$RELEASE_TAG" CHANNEL="$GPROXY_BUILD_CHANNEL" VERSION="$GPROXY_BUILD_VERSION"
export ASSETS_DIR=dist/release ASSET_PREFIX=
if [ "$CHANNEL" = dev ]; then
  export VERSION="$CI_COMMIT_SHA" ASSET_PREFIX="$CI_COMMIT_SHA-"
  scripts/namespace-nightly-assets.sh dist/release "$ASSET_PREFIX"
fi
for platform in github gitlab cnb; do
  export REPO=LeenHawk/gproxy
  case "$platform" in
    github)
      export ASSET_BASE_URL="https://github.com/$REPO/releases/download/$TAG"
      export NOTES_URL="https://github.com/$REPO/releases/tag/$TAG"
      ;;
    gitlab)
      export ASSET_BASE_URL="$CI_PROJECT_URL/-/releases/$TAG/downloads"
      export NOTES_URL="$CI_PROJECT_URL/-/releases/$TAG"
      ;;
    cnb)
      export ASSET_BASE_URL="https://cnb.cool/$REPO/-/releases/download/$TAG"
      export NOTES_URL="https://cnb.cool/$REPO/-/releases/tag/$TAG"
      ;;
  esac
  export OUT="dist/manifests/$platform/manifest.json"
  scripts/build-update-manifest.sh
done
bash scripts/gitlab/publish-containers.sh
python3 scripts/gitlab/publish.py
