#!/usr/bin/env bash
# Run only in an isolated checkout with the pinned tauri-harmony toolchain.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
: "${OHOS_TAURI_SOURCES:?Use the pinned HarmonyOS toolchain image}"
: "${HARMONY_TOOLS_DIR:?Use the pinned HarmonyOS toolchain image}"
python3 scripts/mobile/version.py >/dev/null
export GPROXY_BUILD_VERSION="$(bash scripts/release-metadata.sh version)"
export GPROXY_BUILD_CHANNEL=release
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
export GPROXY_OHOS_DISTRIBUTION=appgallery
export TARGET_TRIPLE=aarch64-unknown-linux-ohos
export OHOS_HOME="$HARMONY_TOOLS_DIR/command-line-tools/sdk/default/openharmony"
unset GPROXY_UPDATE_PUBKEY
pnpm --dir console build
python3 scripts/ohos/prepare-tauri.py
source scripts/ohos/env.sh
export PATH="$HARMONY_TOOLS_DIR/command-line-tools/tool/node/bin:$HARMONY_TOOLS_DIR/command-line-tools/bin:$PATH"
(
  cd crates/gproxy-host-tauri
  cargo tauri ohos init --ci --skip-targets-install
)
python3 scripts/ohos/configure-hap.py
(
  cd crates/gproxy-host-tauri
  cargo tauri ohos build --ci --target aarch64 --ignore-version-mismatches -- --lib
  cd gen/ohos
  GPROXY_OHOS_REUSE_NATIVE=1 hvigorw --mode project assembleApp -p product=default -p buildMode=release --no-daemon
)
python3 scripts/mobile/collect-ohos.py
