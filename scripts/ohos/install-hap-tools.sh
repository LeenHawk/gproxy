#!/usr/bin/env bash
set -euo pipefail
: "${HARMONY_TOOLS_DIR:?}"
# Mirror of Huawei Command Line Tools 6.0.0 Release; both parts are pinned.
if [ ! -f "$HARMONY_TOOLS_DIR/command-line-tools/bin/ohpm" ]; then
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  base=https://github.com/ErBWs/ohos-sdk/releases/download/6.0.0.858/ohos-sdk-linux-amd64.tar.gz
  curl --fail --location --retry 3 "$base.aa" -o "$work/sdk.aa"
  curl --fail --location --retry 3 "$base.ab" -o "$work/sdk.ab"
  printf '%s  %s\n' ce8e66b9b4a8c0f492e348595173cbccd685ed029e33eab4097d6affc184b4de "$work/sdk.aa" \
    785590e57b730333fdc9b71167dd7a32cb9341d88a7625704960b52c6dc33a35 "$work/sdk.ab" | sha256sum -c -
  mkdir -p "$HARMONY_TOOLS_DIR"
  cat "$work/sdk.aa" "$work/sdk.ab" | tar -xz -C "$HARMONY_TOOLS_DIR"
fi
sdk="$HARMONY_TOOLS_DIR/command-line-tools/sdk"
printf '%s\n' "$HARMONY_TOOLS_DIR/command-line-tools/tool/node/bin" "$HARMONY_TOOLS_DIR/command-line-tools/bin" >> "$GITHUB_PATH"
printf 'OHOS_HOME=%s\nOHOS_NDK_HOME=%s\nDEVECO_SDK_HOME=%s\n' "$sdk/default/openharmony" "$sdk/default/openharmony" "$sdk" >> "$GITHUB_ENV"
