#!/usr/bin/env python3
"""Strip and optionally pack staged mobile libraries before packaging and signing."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directory", type=Path)
parser.add_argument("strip_tool", type=Path)
parser.add_argument("--android", action="store_true")
parser.add_argument("--strip-only", action="store_true", help="Preserve the ELF layout for OHOS shared-library loading")
args = parser.parse_args()
libraries = list(args.directory.rglob("*.so"))
if not any(library.name == "libgproxy_host_tauri.so" for library in libraries):
    raise ValueError(f"Application library not found in {args.directory}")
with tempfile.TemporaryDirectory(prefix="gproxy-native-upx-") as work:
    for library in libraries:
        pack = library.name == "libgproxy_host_tauri.so" and not args.strip_only
        # AGP can retain an already-packed ABI in incremental task outputs.
        # Never strip it again; validate it before reusing the staged bytes.
        if pack and subprocess.run(
            ["upx", "--list", library], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        ).returncode == 0:
            subprocess.run(["upx", "--test", library], check=True)
            continue
        packed = Path(work) / library.name
        shutil.copyfile(library, packed)
        packed.chmod(0o755)
        subprocess.run([args.strip_tool, "--strip-unneeded", packed], check=True)
        if pack:
            options = ["--android-shlib"] if args.android else []
            subprocess.run(["upx", "--best", "--lzma", *options, packed], check=True)
            subprocess.run(["upx", "--test", packed], check=True)
        # Replace staged links instead of changing Cargo's cached library.
        library.unlink()
        shutil.copy2(packed, library)
