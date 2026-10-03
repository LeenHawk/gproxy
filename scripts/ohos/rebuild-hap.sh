#!/usr/bin/env bash
# Reassemble a device-test HAP while keeping the supplied native application unchanged.
set -euo pipefail
source_hap="$(realpath "${1:?original unsigned HAP}")"
: "${SOURCE_LIBRARY_SHA256:?}" "${TARGET_TRIPLE:?}" "${ARTIFACT_NAME:?}"
export OHOS_HOME="$HARMONY_TOOLS_DIR/command-line-tools/sdk/default/openharmony"
source scripts/ohos/env.sh
export PATH="$HARMONY_TOOLS_DIR/command-line-tools/tool/node/bin:$HARMONY_TOOLS_DIR/command-line-tools/bin:$PATH"
python3 scripts/ohos/prepare-tauri.py
(cd crates/gproxy-host-tauri && cargo tauri ohos init --ci --skip-targets-install)
python3 scripts/ohos/configure-hap.py
python3 - "$source_hap" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import zipfile

abi = os.environ["OHOS_ARCH"]
entry = Path("crates/gproxy-host-tauri/gen/ohos/entry")
with zipfile.ZipFile(sys.argv[1]) as archive:
    app = json.loads(archive.read("module.json"))["app"]
    configured = json.loads(Path("crates/gproxy-host-tauri/gen/ohos/AppScope/app.json5").read_text())["app"]
    for key in ("bundleName", "versionName", "versionCode"):
        if app[key] != configured[key]:
            raise ValueError(f"Repack must preserve {key}: {app[key]} != {configured[key]}")
    library = f"libs/{abi}/libgproxy_host_tauri.so"
    if hashlib.sha256(archive.read(library)).hexdigest() != os.environ["SOURCE_LIBRARY_SHA256"]:
        raise ValueError("Source application library does not match the device reproduction")
    for name in archive.namelist():
        if name.startswith(f"libs/{abi}/") and name.endswith(".so"):
            # Hvigor rebuilds this template shim; copying it would duplicate it.
            if Path(name).name == "libentry.so":
                continue
            path = entry / "libs" / abi / Path(name).name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(archive.read(name))
path = entry / "libs" / abi / "libgproxy_host_tauri.so"
subprocess.run(["upx", "-d", str(path)], check=True)
print("Reused unpacked native SHA256:", hashlib.sha256(path.read_bytes()).hexdigest())
PY
(
  cd crates/gproxy-host-tauri/gen/ohos
  ohpm install --all
  GPROXY_OHOS_REUSE_NATIVE=1 hvigorw --mode module assembleHap -p buildMode=release --no-daemon
)
python3 scripts/ohos/package-hap.py
