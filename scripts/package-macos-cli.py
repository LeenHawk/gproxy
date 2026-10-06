#!/usr/bin/env python3
"""Put the actual CLI and installation instructions in a macOS disk image."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def package(binary: Path, artifact: str, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        shutil.copy2(binary, root / "gproxy")
        (root / "gproxy").chmod(0o755)
        for name in ("README.md", "LICENSE"):
            shutil.copy2(name, root / name)
        (root / "INSTALL.txt").write_text(
            "GPROXY CLI / server\n\n"
            "In Terminal, install the command with:\n"
            "  sudo mkdir -p /usr/local/bin\n"
            '  sudo install -m 755 "/Volumes/GPROXY CLI/gproxy" /usr/local/bin/gproxy\n'
            "  gproxy --help\n\n"
            "This image contains the command-line server. The GPROXY Application\n"
            "has a separate gproxy-tauri package. No service is enabled automatically.\n"
        )
        package_path = output / f"{artifact}.dmg"
        subprocess.run([sys.executable, str(Path(__file__).with_name("reproducible-dmg.py")),
                        "--source", str(root), "--output", str(package_path),
                        "--volume-name", "GPROXY CLI"], check=True)
        with package_path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        Path(str(package_path) + ".sha256").write_text(f"{digest}  {package_path.name}\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--artifact", required=True)
    parser.add_argument("--output-dir", type=Path, default=Path("dist/release"))
    args = parser.parse_args()
    package(args.binary, args.artifact, args.output_dir)
