#!/usr/bin/env python3
"""Validate and collect the configured OHOS Application HAP."""
import hashlib
import os
from pathlib import Path
import struct
import subprocess
import sys
import zipfile

signing = "signed" if os.environ.get("GPROXY_OHOS_SIGNING_CONFIG") else "unsigned"
paths = list(Path("crates/gproxy-host-tauri/gen").glob(f"**/outputs/**/entry-default-{signing}.hap"))
if len(paths) != 1:
    raise ValueError(f"Expected one {signing} HAP, found {len(paths)}")
abi = {"aarch64-unknown-linux-ohos": "arm64-v8a", "x86_64-unknown-linux-ohos": "x86_64"}[os.environ["TARGET_TRIPLE"]]
machine = 183 if abi == "arm64-v8a" else 62
with zipfile.ZipFile(paths[0]) as archive:
    binary = f"libs/{abi}/libgproxy_host_tauri.so"
    with archive.open(binary) as stream:
        header = stream.read(64)
    if len(header) != 64 or header[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", header, 18)[0] != machine:
        raise ValueError(f"Wrong native architecture in HAP: {binary}")
    section_offset = struct.unpack_from("<Q", header, 40)[0]
    section_size, section_count = struct.unpack_from("<HH", header, 58)
    # OHOS maps the section table while loading a DSO; UPX removes it, causing EINVAL.
    if not section_offset or not section_count or section_size != 64 or section_offset + section_size * section_count > archive.getinfo(binary).file_size:
        raise ValueError(f"Missing or invalid ELF section table in HAP: {binary}; do not UPX-pack OHOS shared libraries")
    if "module.json" not in archive.namelist():
        raise ValueError("HAP has no compiled module manifest")
output = Path("dist/release")
output.mkdir(parents=True, exist_ok=True)
package = output / (os.environ["ARTIFACT_NAME"] + ".hap")
package.write_bytes(paths[0].read_bytes())
if signing == "unsigned":
    subprocess.run([sys.executable, "scripts/reproducible-zip-timestamps.py", str(package)], check=True)
with package.open("rb") as stream:
    digest = hashlib.file_digest(stream, "sha256").hexdigest()
Path(str(package) + ".sha256").write_text(f"{digest}  {package.name}\n")
print(f"Verified {signing} HAP native library: {binary}; wrote {package}")
