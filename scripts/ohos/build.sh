#!/usr/bin/env bash
# Run in the shared OHOS toolchain image, from the repository root.
set -euo pipefail
mode="${1:?cli|application}"
case "$mode" in
  cli) export OHOS_HOME="$HARMONY_TOOLS_DIR/command-line-tools/sdk/default/openharmony" BUILDER=cargo-ohos ;;
  application)
    export OHOS_HOME="$HARMONY_TOOLS_DIR/command-line-tools/sdk/default/openharmony"
    export ARTIFACT_NAME="${APPLICATION_ARTIFACT:?}" BUILDER=tauri-ohos
    export CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
    ;;
  *) exit 2 ;;
esac
source scripts/ohos/env.sh
if [ "$mode" = cli ]; then
  export UPX_ENABLED=true
  cargo build --locked --release -p gproxy --bin gproxy --target "$TARGET_TRIPLE"
  bash scripts/ohos/package-cli.sh
else
  export UPX_ENABLED=false
  export PATH="$HARMONY_TOOLS_DIR/command-line-tools/tool/node/bin:$HARMONY_TOOLS_DIR/command-line-tools/bin:$PATH"
  (cd crates/gproxy-host-tauri && cargo tauri ohos init --ci --skip-targets-install)
  python3 scripts/ohos/configure-hap.py
  (cd crates/gproxy-host-tauri && cargo tauri ohos build --ci --target "${TARGET_TRIPLE%%-*}" -- --lib)
  python3 scripts/ohos/package-hap.py
fi
scripts/build-provenance.sh
