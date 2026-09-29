#!/usr/bin/env python3
"""Validate and collect the unsigned OHOS Application HAP."""
import hashlib
import os
from pathlib import Path
import struct
import zipfile

paths = list(Path("crates/gproxy-host-tauri/gen").glob("**/outputs/**/entry-default-unsigned.hap"))
if len(paths) != 1:
    raise ValueError(f"Expected one unsigned HAP, found {len(paths)}")
abi = {"aarch64-unknown-linux-ohos": "arm64-v8a", "x86_64-unknown-linux-ohos": "x86_64"}[os.environ["TARGET_TRIPLE"]]
machine = 183 if abi == "arm64-v8a" else 62
with zipfile.ZipFile(paths[0]) as archive:
    binary = f"libs/{abi}/libgproxy_host_tauri.so"
    header = archive.read(binary)[:20]
    if header[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", header, 18)[0] != machine:
        raise ValueError(f"Wrong native architecture in HAP: {binary}")
    if "module.json" not in archive.namelist():
        raise ValueError("HAP has no compiled module manifest")
output = Path("dist/release")
output.mkdir(parents=True, exist_ok=True)
package = output / (os.environ["ARTIFACT_NAME"] + ".hap")
package.write_bytes(paths[0].read_bytes())
with package.open("rb") as stream:
    digest = hashlib.file_digest(stream, "sha256").hexdigest()
Path(str(package) + ".sha256").write_text(f"{digest}  {package.name}\n")
print(f"Verified unsigned HAP native library: {binary}; wrote {package}")
