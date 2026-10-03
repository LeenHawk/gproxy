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
# Keep the successful device A/B library byte-identical, including its ELF metadata.
profile = entry / "build-profile.json5"
settings = json.loads(profile.read_text())
for mode in settings["buildOptionSet"]:
    if mode["name"] == "release":
        mode.setdefault("nativeLib", {}).setdefault("debugSymbol", {})["strip"] = False
profile.write_text(json.dumps(settings, indent=2) + "\n")
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
digest = hashlib.sha256(path.read_bytes()).hexdigest()
Path("target/ohos-startup-probe").mkdir(parents=True, exist_ok=True)
Path("target/ohos-startup-probe/native.sha256").write_text(digest)
print("Reused unpacked native SHA256:", digest)
PY
(
  cd crates/gproxy-host-tauri/gen/ohos
  ohpm install --all
  GPROXY_OHOS_REUSE_NATIVE=1 hvigorw --mode module assembleHap -p buildMode=release --no-daemon
)
python3 scripts/ohos/package-hap.py
python3 - <<'PY'
import hashlib
import os
from pathlib import Path
import zipfile

hap = Path("dist/release") / (os.environ["ARTIFACT_NAME"] + ".hap")
with zipfile.ZipFile(hap) as archive:
    library = archive.read(f'libs/{os.environ["OHOS_ARCH"]}/libgproxy_host_tauri.so')
expected = Path("target/ohos-startup-probe/native.sha256").read_text()
if hashlib.sha256(library).hexdigest() != expected:
    raise ValueError("Packaging changed the native A/B control library")
print("Verified byte-identical native library in the startup probe HAP")
PY
