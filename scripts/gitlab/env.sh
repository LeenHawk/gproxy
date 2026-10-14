#!/usr/bin/env bash
export GPROXY_BUILD_HASH="${CI_COMMIT_SHA:?}"
release_context="$(python3 scripts/release-context.py --shell)" || return 1
eval "$release_context"
export GPROXY_BUILD_UPDATE_SOURCE=github
export GPROXY_UPDATE_PUBKEY="$(cat .gitlab/update-public-key)"
export GPROXY_INSTALLATION_KIND=standalone
export RUNNER_TEMP="$CI_PROJECT_DIR/.ci-tmp"
mkdir -p "$RUNNER_TEMP"
export GITHUB_PATH="$RUNNER_TEMP/paths"
touch "$GITHUB_PATH"
export GITHUB_SHA="$CI_COMMIT_SHA" GITHUB_REF_NAME="$RELEASE_TAG" GPROXY_VERSION="$GPROXY_BUILD_VERSION"
if [ -n "${TARGET_TRIPLE:-}" ]; then
  row="$(jq -ce --arg target "$TARGET_TRIPLE" '.include[] | select(.target==$target)' scripts/release-targets.json)"
  export TARGET_OS="$(jq -r .os <<< "$row")" ARTIFACT_NAME="$(jq -r .artifact <<< "$row")"
  export APPLICATION_ARTIFACT="$(jq -r '.application_artifact // empty' <<< "$row")"
  export BUILDER="$(jq -r .builder <<< "$row")" UPX_ENABLED="$(jq -r .upx <<< "$row")"
  export UPX_VERSION="$(jq -r '.upx_version // "5.2.1"' <<< "$row")"
  export NDK_TARGET="$(jq -r '.ndk_target // empty' <<< "$row")"
fi
release_tool_paths() {
  while IFS= read -r directory; do
    directory="${directory%$'\r'}"
    if [[ "$directory" == [A-Za-z]:* ]]; then directory="$(cygpath -u "$directory")"; fi
    export PATH="$directory:$PATH"
  done < "$GITHUB_PATH"
}
