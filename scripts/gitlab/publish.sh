#!/usr/bin/env bash
set -euo pipefail
source scripts/gitlab/env.sh
: "${GH_TOKEN:?}" "${GITLAB_RELEASE_TOKEN:?}" "${CNB_TOKEN:?}"
test "$GPROXY_UPDATE_PUBKEY" = "$UPDATE_SIGNING_PUBLIC_KEY_B64"
if [ -n "$GPROXY_PUBLISH_BRANCH" ]; then
  latest="$(git ls-remote "https://github.com/$GITHUB_REPOSITORY.git" "refs/heads/$GPROXY_PUBLISH_BRANCH" | cut -f1)"
  if [ "$latest" != "$CI_COMMIT_SHA" ]; then
    echo "Skipping stale build: $GPROXY_PUBLISH_BRANCH has advanced."
    exit 0
  fi
fi
if [ "${GITHUB_ACTIONS:-false}" != true ]; then
  cp dist/msix/* dist/release/
  cp dist/dmg/* dist/release/
fi
VERIFY_ONLY=true python3 scripts/gitlab/publish.py
export TAG="$RELEASE_TAG" CHANNEL="$GPROXY_BUILD_CHANNEL" VERSION="$GPROXY_MANIFEST_VERSION"
export ASSETS_DIR=dist/release
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
  if [ "$CHANNEL" = release ]; then
    CHANNEL=beta OUT="dist/manifests/$platform/beta/manifest.json" scripts/build-update-manifest.sh
  fi
done
if [ "${GITHUB_ACTIONS:-false}" = true ]; then
  bash scripts/github/mirror-containers.sh
else
  bash scripts/gitlab/publish-containers.sh
fi
python3 scripts/gitlab/publish.py
