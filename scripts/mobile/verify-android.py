#!/usr/bin/env python3
"""Inspect Android channel policy, expected ABIs and 16 KiB ELF alignment."""
import argparse
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
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("package", type=Path)
parser.add_argument("--distribution", choices=("direct", "fdroid", "google-play", "appgallery"), default="fdroid")
parser.add_argument("--abis", nargs="+", choices=("arm64-v8a", "x86_64"))
args = parser.parse_args()
apk = args.package.resolve()
is_bundle = apk.suffix == ".aab"
store = args.distribution != "direct"
if not is_bundle:
    sdk = Path(os.environ.get("ANDROID_HOME") or os.environ["ANDROID_SDK_ROOT"])
    aapt = sdk / "build-tools/36.1.0/aapt2"
    badging = subprocess.check_output([aapt, "dump", "badging", apk], text=True)
    identity = re.search(r"package: name='([^']+)' versionCode='([^']+)' versionName='([^']+)'", badging)
    if not identity or identity.group(1) != "com.leenhawk.gproxy.app":
        raise ValueError("Unexpected APK identity")
    if store:
        config = json.loads(subprocess.check_output([sys.executable, root / "scripts/mobile/version.py"], text=True))
        expected = ("com.leenhawk.gproxy.app", str(config["bundle"]["android"]["versionCode"]), config["version"])
        if identity.groups() != expected:
            raise ValueError(f"APK identity/version does not match {expected}")
    elif identity.group(3) != os.environ["GPROXY_BUILD_VERSION"]:
        raise ValueError("Direct APK version does not match the release")
    if "application-debuggable" in badging:
        raise ValueError("Release APK must not be debuggable")
    manifest = subprocess.check_output([aapt, "dump", "xmltree", apk, "--file", "AndroidManifest.xml"], text=True)
    updater = ("REQUEST_INSTALL_PACKAGES", "GproxyUpdateActivity", "GproxyUpdateProvider")
    if store:
        for forbidden in (*updater, "FOREGROUND_SERVICE_DATA_SYNC"):
            if forbidden in manifest:
                raise ValueError(f"Store manifest still includes {forbidden}")
        if "GproxyPrivacyActivity" not in manifest:
            raise ValueError("Store APK is missing its offline privacy notice")
    elif any(required not in manifest for required in updater):
        raise ValueError("Direct APK is missing its updater or installation permission")
    if "launchable-activity: name='com.leenhawk.gproxy.app.MainActivity'" not in badging:
        raise ValueError("APK does not launch the application directly")
# The paired APK above verifies the Google Play manifest. AAB native payloads
# must independently contain the same requested architectures and store policy.
ndk = Path(os.environ["ANDROID_NDK_HOME"])
nm = next(ndk.glob("toolchains/llvm/prebuilt/*/bin/llvm-nm"))
with zipfile.ZipFile(apk) as archive, tempfile.TemporaryDirectory(prefix="gproxy-store-elf-") as work:
    prefix = "base/lib/" if is_bundle else "lib/"
    libraries = [name for name in archive.namelist() if name.startswith(prefix) and name.endswith(".so")]
    app_abis = {name[len(prefix):].split("/")[0] for name in libraries if name.endswith("/libgproxy_host_tauri.so")}
    if not app_abis:
        raise ValueError("Package is missing the application library")
    if args.abis and app_abis != set(args.abis):
        raise ValueError(f"Expected ABIs {args.abis}, found {sorted(app_abis)}")
    for name in libraries:
        data = archive.read(name)
        # Both supported ABIs use little-endian ELF64.
        if data[:6] != b"\x7fELF\x02\x01":
            raise ValueError(f"Unexpected ELF format: {name}")
        abi = name[len(prefix):].split("/")[0]
        if abi not in app_abis or struct.unpack_from("<H", data, 18)[0] != {"arm64-v8a": 183, "x86_64": 62}.get(abi):
            raise ValueError(f"Native library architecture mismatch: {name}")
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
            has_updater = "Java_com_leenhawk_gproxy_app_GproxyNative_nativeUpdate" in symbols
            if has_updater == store:
                raise ValueError(f"Native updater does not match {args.distribution} policy: {name}")
print(f"Verified {apk.name}: {args.distribution}, ABIs {sorted(app_abis)}, {len(libraries)} aligned native libraries")
