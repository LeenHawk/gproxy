#!/usr/bin/env bash
# Source this in each CNB stage. Build identity and update host are public.
export GPROXY_BUILD_VERSION="$(scripts/release-metadata.sh version)"
export GPROXY_BUILD_HASH="${CNB_COMMIT:?}"
export GPROXY_BUILD_UPDATE_SOURCE=cnb
export GPROXY_UPDATE_PUBKEY="$(cat .cnb/update-public-key)"
export GPROXY_BUILD_CHANNEL=dev RELEASE_TAG=nightly
if [[ "${CNB_BRANCH:?}" == v* ]]; then
  scripts/release-metadata.sh verify-tag "$CNB_BRANCH"
  export RELEASE_TAG="$CNB_BRANCH" GPROXY_BUILD_CHANNEL=release
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
