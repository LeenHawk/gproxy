#!/usr/bin/env python3
"""Repack a Tauri DEB with source-derived times and root ownership."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
if not os.environ.get("SOURCE_DATE_EPOCH"):
    parser.error("SOURCE_DATE_EPOCH is required")
with tempfile.TemporaryDirectory(prefix="gproxy-deb-") as directory:
    payload = Path(directory) / "package"
    subprocess.run(["dpkg-deb", "--raw-extract", args.source, payload], check=True)
    payload.chmod(0o755)
    subprocess.run([sys.executable, Path(__file__).with_name("reproducible-env.py"),
                    "--normalize-tree", payload], check=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["dpkg-deb", "--root-owner-group", "-Zgzip", "-z9",
                    "--build", payload, args.output], check=True)
