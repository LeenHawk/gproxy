#!/usr/bin/env bash
# Source this in each CNB stage. Build identity and update host are public.
export GPROXY_BUILD_VERSION="$(scripts/release-metadata.sh version)"
export GPROXY_BUILD_HASH="${CNB_COMMIT:?}"
export GPROXY_BUILD_UPDATE_SOURCE=cnb
export GPROXY_UPDATE_PUBKEY="$(cat .cnb/update-public-key)"
export GPROXY_BUILD_CHANNEL=dev RELEASE_TAG=nightly
export GPROXY_SOURCE_VERSION="$GPROXY_BUILD_VERSION" GPROXY_PUBLISH_BRANCH=dev
if [[ "${CNB_BRANCH:?}" == v* ]]; then
  scripts/release-metadata.sh verify-tag "$CNB_BRANCH"
  export RELEASE_TAG="$CNB_BRANCH" GPROXY_BUILD_CHANNEL=release
  export GPROXY_PUBLISH_BRANCH=
  [[ "$GPROXY_BUILD_VERSION" != *-* ]] || export GPROXY_BUILD_CHANNEL=beta
elif [ "$CNB_BRANCH" != dev ]; then
  echo "CNB releases require dev or a matching version tag" >&2
  return 1
fi
export GPROXY_INSTALLATION_KIND=standalone
export RUNNER_TEMP="${TMPDIR:-/tmp}/gproxy-cnb"
mkdir -p "$RUNNER_TEMP"
export GITHUB_PATH="$RUNNER_TEMP/paths"
touch "$GITHUB_PATH"
# Existing packaging scripts use these only for provenance and tool paths.
export GITHUB_SHA="$CNB_COMMIT" GITHUB_REF_NAME="$RELEASE_TAG"
export GPROXY_VERSION="$GPROXY_BUILD_VERSION"

cnb_tool_paths() {
  while IFS= read -r directory; do export PATH="$directory:$PATH"; done < "$GITHUB_PATH"
}

# Shared mirror publisher identity. Compilation remains entirely on CNB.
export CI_COMMIT_SHA="$CNB_COMMIT" CI_COMMIT_TAG=
[ "$GPROXY_BUILD_CHANNEL" = dev ] || export CI_COMMIT_TAG="$RELEASE_TAG"
export CI_PROJECT_DIR="$PWD" CI_PROJECT_ID=86976712
export CI_API_V4_URL=https://gitlab.com/api/v4
export CI_PROJECT_URL=https://gitlab.com/leenhawk1/gproxy
export CI_PIPELINE_ID="$CNB_BUILD_ID"
export CI_PIPELINE_URL="https://cnb.cool/$CNB_REPO_SLUG/-/build/logs/$CNB_BUILD_ID"
export GITHUB_REPOSITORY=LeenHawk/gproxy
export CI_REGISTRY=registry.gitlab.com CI_REGISTRY_IMAGE=registry.gitlab.com/leenhawk1/gproxy
export CI_REGISTRY_USER=oauth2 CI_REGISTRY_PASSWORD="${GITLAB_RELEASE_TOKEN:-}"
