#!/usr/bin/env bash
# Restore caches on the runner, build at the recipe's fixed paths in one image.
set -euo pipefail
: "${ANDROID_TOOLCHAIN_IMAGE:?}"
canonical=/tmp/gproxy-reproduce
mkdir -p "$canonical/source"
tar --exclude='./target' --exclude='./dist' --exclude='node_modules' -cf - . | tar -xf - -C "$canonical/source"
if [ -d dist/android-build ]; then
  mkdir -p "$canonical/source/dist"
  cp -R dist/android-build "$canonical/source/dist/"
fi
restore_owner() {
  docker run --rm --volume "$canonical:$canonical" "$ANDROID_TOOLCHAIN_IMAGE" \
    chown -R "$(id -u):$(id -g)" "$canonical"
}
trap restore_owner EXIT
docker run --rm --volume "$canonical:$canonical" --workdir "$canonical/source" \
  --env CARGO_HOME --env CARGO_TARGET_DIR --env GRADLE_USER_HOME --env CARGO_BUILD_JOBS \
  --env CARGO_INCREMENTAL=0 --env SOURCE_DATE_EPOCH --env GPROXY_BUILD_HASH \
  --env GPROXY_BUILD_VERSION --env GPROXY_BUILD_CHANNEL --env GPROXY_UPDATE_PUBKEY \
  --env GITHUB_SHA --env GITHUB_REF_NAME \
  --env ANDROID_SIGNING_KEYSTORE_B64 --env ANDROID_SIGNING_KEYSTORE_PASSWORD \
  --env ANDROID_SIGNING_KEY_ALIAS --env ANDROID_SIGNING_KEY_PASSWORD \
  --env GOOGLE_PLAY_UPLOAD_KEYSTORE_B64 --env GOOGLE_PLAY_UPLOAD_STORE_PASSWORD \
  --env GOOGLE_PLAY_UPLOAD_KEY_ALIAS --env GOOGLE_PLAY_UPLOAD_KEY_PASSWORD \
  --env GIT_CONFIG_COUNT=1 --env GIT_CONFIG_KEY_0=safe.directory --env "GIT_CONFIG_VALUE_0=$canonical/source" \
  "$ANDROID_TOOLCHAIN_IMAGE" bash scripts/mobile/release-android.sh "$@"
restore_owner
trap - EXIT
mkdir -p dist
if [ "$1" = build ]; then
  cp -R "$canonical/source/dist/android-build" dist/
else
  cp -R "$canonical/source/dist/release" dist/
fi
