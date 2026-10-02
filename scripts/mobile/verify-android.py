#!/usr/bin/env python3
"""Inspect the built store APK: identity, updater removal and 16 KiB ELF alignment."""
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile
import zipfile

root = Path(__file__).resolve().parents[2]
apk = Path(sys.argv[1]).resolve()
sdk = Path(os.environ.get("ANDROID_HOME") or os.environ["ANDROID_SDK_ROOT"])
aapt = sorted(sdk.glob("build-tools/*/aapt2"), key=lambda p: tuple(int(n) for n in p.parent.name.split(".")))[-1]
config = json.loads(subprocess.check_output([sys.executable, root / "scripts/mobile/version.py"], text=True))
badging = subprocess.check_output([aapt, "dump", "badging", apk], text=True)
expected = ("com.leenhawk.gproxy.app", str(config["bundle"]["android"]["versionCode"]), config["version"])
identity = re.search(r"package: name='([^']+)' versionCode='([^']+)' versionName='([^']+)'", badging)
if not identity or identity.groups() != expected:
    raise ValueError(f"APK identity/version does not match {expected}")
if "application-debuggable" in badging:
    raise ValueError("Store APK must not be debuggable")
manifest = subprocess.check_output([aapt, "dump", "xmltree", apk, "--file", "AndroidManifest.xml"], text=True)
for forbidden in ("REQUEST_INSTALL_PACKAGES", "FOREGROUND_SERVICE_DATA_SYNC", "GproxyUpdateActivity", "GproxyUpdateProvider"):
    if forbidden in manifest:
        raise ValueError(f"Store manifest still includes {forbidden}")
if "GproxyPrivacyActivity" not in manifest:
    raise ValueError("Store APK is missing its privacy launcher")
ndk = Path(os.environ["ANDROID_NDK_HOME"])
nm = next(ndk.glob("toolchains/llvm/prebuilt/*/bin/llvm-nm"))
with zipfile.ZipFile(apk) as archive, tempfile.TemporaryDirectory(prefix="gproxy-store-elf-") as work:
    libraries = [name for name in archive.namelist() if name.startswith("lib/") and name.endswith(".so")]
    if not any(name.endswith("/libgproxy_host_tauri.so") for name in libraries):
        raise ValueError("APK is missing the application library")
    for name in libraries:
        data = archive.read(name)
        # Both supported ABIs use little-endian ELF64.
        if data[:6] != b"\x7fELF\x02\x01":
            raise ValueError(f"Unexpected ELF format: {name}")
        offset = struct.unpack_from("<Q", data, 32)[0]
        size, count = struct.unpack_from("<HH", data, 54)
        loads = 0
        for index in range(count):
            header = struct.unpack_from("<IIQQQQQQ", data, offset + index * size)
            if header[0] == 1:
                loads += 1
                if header[7] < 16384 or (header[2] - header[3]) % 16384:
                    raise ValueError(f"PT_LOAD is not 16 KiB aligned: {name}")
        if not loads:
            raise ValueError(f"ELF has no loadable segments: {name}")
        if name.endswith("/libgproxy_host_tauri.so"):
            library = Path(work) / "libgproxy_host_tauri.so"
            library.write_bytes(data)
            symbols = subprocess.check_output([nm, "--dynamic", "--defined-only", library], text=True)
            if "Java_com_leenhawk_gproxy_app_GproxyNative_nativeUpdate" in symbols:
                raise ValueError("Store library still exports the APK download entry point")
print(f"Verified {expected}: no APK updater, release manifest, {len(libraries)} aligned native libraries")
