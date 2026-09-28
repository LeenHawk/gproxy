#!/usr/bin/env python3
"""Package the actual Tauri executable as an independent macOS application."""
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile

target = os.environ["TARGET_TRIPLE"]
artifact = os.environ["ARTIFACT_NAME"]
version = os.environ["GPROXY_BUILD_VERSION"]
root = Path.cwd()
with tempfile.TemporaryDirectory() as temporary:
    app = Path(temporary) / "GPROXY.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    shutil.copy2(root / f"target/{target}/release/gproxy-desktop", contents / "MacOS/gproxy-desktop")
    shutil.copy2(root / "crates/gproxy-host-tauri/icons/icon.icns", contents / "Resources/icon.icns")
    with (contents / "Info.plist").open("wb") as stream:
        plistlib.dump({
            "CFBundleDevelopmentRegion": "en", "CFBundleExecutable": "gproxy-desktop",
            "CFBundleIdentifier": "dev.gproxy.desktop", "CFBundleInfoDictionaryVersion": "6.0",
            "CFBundleName": "GPROXY", "CFBundleDisplayName": "GPROXY", "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": version.split("-")[0], "CFBundleVersion": version.split("-")[0],
            "CFBundleIconFile": "icon.icns", "NSHighResolutionCapable": True,
            "LSMinimumSystemVersion": os.environ["MACOSX_DEPLOYMENT_TARGET"],
            "NSAppTransportSecurity": {"NSAllowsLocalNetworking": True},
        }, stream)
    subprocess.run(["rcodesign", "sign", str(app)], check=True)
    subprocess.run(["python3", "scripts/cnb/verify-macho.py", str(contents / "MacOS/gproxy-desktop")], check=True)
    subprocess.run(["zip", "-9", "-q", "-r", str(root / f"dist/release/{artifact}.app.zip"), "GPROXY.app"], cwd=temporary, check=True)
