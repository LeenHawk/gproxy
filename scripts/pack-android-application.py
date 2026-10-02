#!/usr/bin/env python3
"""Strip native debug data and pack the Rust library before alignment/signing."""

import pathlib
import subprocess
import sys
import tempfile
import zipfile


def pack(source: str, destination: str, strip_tool: str) -> None:
    with tempfile.TemporaryDirectory(prefix="gproxy-apk-upx-") as directory:
        with zipfile.ZipFile(source) as original, zipfile.ZipFile(destination, "w") as packed:
            libraries = 0
            for entry in original.infolist():
                data = original.read(entry)
                if entry.filename.startswith("lib/") and entry.filename.endswith(".so"):
                    library = pathlib.Path(directory) / pathlib.Path(entry.filename).name
                    library.write_bytes(data)
                    library.chmod(0o755)
                    subprocess.run([strip_tool, "--strip-unneeded", str(library)], check=True)
                    if library.name == "libgproxy_host_tauri.so":
                        subprocess.run(
                            ["upx", "--best", "--lzma", "--android-shlib", str(library)],
                            check=True,
                        )
                        subprocess.run(["upx", "--test", str(library)], check=True)
                        libraries += 1
                    data = library.read_bytes()
                packed.writestr(entry, data)
            if libraries == 0:
                raise RuntimeError("APK does not contain libgproxy_host_tauri.so")


if __name__ == "__main__":
    pack(sys.argv[1], sys.argv[2], sys.argv[3])
