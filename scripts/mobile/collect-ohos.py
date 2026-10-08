#!/usr/bin/env python3
"""Inspect the APP's embedded HAPs and collect store/device-test packages."""
import io
import json
import os
from pathlib import Path
import shutil
import struct
import zipfile

root = Path(__file__).resolve().parents[2]
project = root / "crates/gproxy-host-tauri/gen/ohos"
signed = bool(os.environ.get("GPROXY_OHOS_SIGNING_CONFIG"))
suffix = "-signed.app" if signed else "-unsigned.app"
packages = [p for p in project.glob("build/outputs/**/*.app") if p.name.endswith(suffix)]
if len(packages) != 1:
    raise ValueError(f"Expected one {suffix} bundle, found {len(packages)}")
expected = json.loads((root / "crates/gproxy-host-tauri/tauri.conf.json").read_text())
store = json.loads((root / "distribution/mobile/tauri.store.conf.json").read_text())
output = root / "dist/mobile/appgallery-ohos"
output.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(packages[0]) as app:
    if "pack.info" not in app.namelist():
        raise ValueError("APP bundle is missing pack.info")
    haps = [name for name in app.namelist() if name.endswith(".hap")]
    if not haps:
        raise ValueError("APP bundle contains no HAP")
    for name in haps:
        data = app.read(name)
        with zipfile.ZipFile(io.BytesIO(data)) as hap:
            manifest = json.loads(hap.read("module.json"))
            identity = manifest["app"]
            if (identity["bundleName"], identity["versionName"], identity["versionCode"]) != (
                expected["identifier"], expected["version"], store["bundle"]["android"]["versionCode"]
            ):
                raise ValueError("HAP identity/version does not match the release")
            if identity.get("debug", False):
                raise ValueError("HAP is debuggable")
            library = hap.read("libs/arm64-v8a/libgproxy_host_tauri.so")
            if len(library) < 64 or library[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", library, 18)[0] != 183:
                raise ValueError("HAP does not contain the ARM64 application")
            section_offset = struct.unpack_from("<Q", library, 40)[0]
            section_size, section_count = struct.unpack_from("<HH", library, 58)
            if not section_offset or not section_count or section_size != 64 or section_offset + section_size * section_count > len(library):
                raise ValueError("OHOS application library must retain its ELF section table; do not UPX-pack it")
            if manifest["module"]["mainElement"] != "EntryAbility":
                raise ValueError("Store HAP does not launch the application directly")
        (output / Path(name).name).write_bytes(data)
destination = output / ("gproxy-appgallery-ohos" + suffix)
shutil.copyfile(packages[0], destination)
print(f"Inspected APP and {len(haps)} embedded HAP(s): {destination}")
if not signed:
    print("Unsigned preparation output; a release certificate/profile and device validation are still required.")
