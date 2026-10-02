#!/usr/bin/env python3
"""Check the store version against the workspace; print Tauri's stable config."""
import json
from pathlib import Path
import re
import tomllib

root = Path(__file__).resolve().parents[2]
version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
if not re.fullmatch(r"\d+\.\d+\.\d+", version):
    raise ValueError("Store builds require a stable major.minor.patch version")
major, minor, patch = map(int, version.split("."))
code = major * 1_000_000 + minor * 1_000 + patch
if minor >= 1000 or patch >= 1000 or not 1 <= code <= 2_100_000_000:
    raise ValueError("Version is outside the Android versionCode mapping")
config = json.loads((root / "crates/gproxy-host-tauri/tauri.conf.json").read_text())
store = json.loads((root / "distribution/mobile/tauri.store.conf.json").read_text())
if config["version"] != version or store["bundle"]["android"]["versionCode"] != code:
    raise ValueError("Update tauri.conf.json version and the store config's versionCode with Cargo.toml")
print(json.dumps({"version": version, "bundle": {"android": {"versionCode": code}}}))
