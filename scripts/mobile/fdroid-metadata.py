#!/usr/bin/env python3
"""Render an fdroiddata recipe for an immutable source commit after publication."""
import argparse
import json
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("commit", help="Public commit or tag containing the mobile-store build support")
parser.add_argument("--output", type=Path, default=root / "dist/mobile/fdroid/metadata/dev.gproxy.desktop.yml")
args = parser.parse_args()
commit = subprocess.check_output(["git", "rev-parse", "--verify", f"{args.commit}^{{commit}}"], cwd=root, text=True).strip()
subprocess.run(["git", "cat-file", "-e", f"{commit}:scripts/mobile/build-android.sh"], cwd=root, check=True)
config = json.loads(subprocess.check_output(["git", "show", f"{commit}:crates/gproxy-host-tauri/tauri.conf.json"], cwd=root, text=True))
store = json.loads(subprocess.check_output(["git", "show", f"{commit}:distribution/mobile/tauri.store.conf.json"], cwd=root, text=True))
if not re.fullmatch(r"\d+\.\d+\.\d+", config["version"]):
    raise ValueError("F-Droid submission requires a stable version")
recipe = (root / "distribution/fdroid/dev.gproxy.desktop.yml.in").read_text()
for key, value in {"COMMIT": commit, "VERSION": config["version"], "CODE": store["bundle"]["android"]["versionCode"]}.items():
    recipe = recipe.replace(f"@{key}@", str(value))
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(recipe)
print(args.output)
