#!/usr/bin/env python3
"""Pack the app's Rust library before APK alignment and signing."""

import pathlib
import subprocess
import sys
import tempfile
import zipfile


def pack(source: str, destination: str) -> None:
    with tempfile.TemporaryDirectory(prefix="gproxy-apk-upx-") as directory:
        with zipfile.ZipFile(source) as original, zipfile.ZipFile(destination, "w") as packed:
            libraries = 0
            for entry in original.infolist():
                data = original.read(entry)
                if entry.filename.startswith("lib/") and entry.filename.endswith(
                    "/libgproxy_host_tauri.so"
                ):
                    library = pathlib.Path(directory) / "libgproxy_host_tauri.so"
                    library.write_bytes(data)
                    library.chmod(0o755)
                    subprocess.run(
                        ["upx", "--best", "--lzma", "--android-shlib", str(library)],
                        check=True,
                    )
                    subprocess.run(["upx", "--test", str(library)], check=True)
                    data = library.read_bytes()
                    libraries += 1
                packed.writestr(entry, data)
            if libraries == 0:
                raise RuntimeError("APK does not contain libgproxy_host_tauri.so")


if __name__ == "__main__":
    pack(sys.argv[1], sys.argv[2])
