#!/usr/bin/env bash
# Run in the shared OHOS toolchain image, from the repository root.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../reproducible-env.sh"
[[ "${1:-}" == application && "$TARGET_TRIPLE" == aarch64-unknown-linux-ohos ]] || {
  echo "Expected application build for aarch64-unknown-linux-ohos" >&2
  exit 2
}
export OHOS_HOME="$HARMONY_TOOLS_DIR/command-line-tools/sdk/default/openharmony"
export ARTIFACT_NAME="${APPLICATION_ARTIFACT:?}" BUILDER=tauri-ohos UPX_ENABLED=false
export CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
source scripts/ohos/env.sh
export PATH="$HARMONY_TOOLS_DIR/command-line-tools/tool/node/bin:$HARMONY_TOOLS_DIR/command-line-tools/bin:$PATH"
(cd crates/gproxy-host-tauri && cargo tauri ohos init --ci --skip-targets-install)
python3 scripts/ohos/configure-hap.py
(cd crates/gproxy-host-tauri && python3 "$OLDPWD/scripts/reproducible-run.py" cargo tauri ohos build --ci --target "${TARGET_TRIPLE%%-*}" -- --lib)
python3 scripts/ohos/package-hap.py
scripts/build-provenance.sh
