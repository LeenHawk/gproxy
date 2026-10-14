#!/usr/bin/env python3
"""Derive reproducibility coverage from the published release target inventory."""
import argparse
import json
import sys
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--target", default="all")
parser.add_argument("--kind", default="all")
parser.add_argument("--containers", action="store_true")
args = parser.parse_args()
rows = json.loads((Path(__file__).with_name("release-targets.json")).read_text())["include"]
if args.containers:
    print(json.dumps({"include": [{"target": row["target"]} for row in rows if row["os"] == "linux"]}))
    sys.exit(0)
matrix = []
for row in rows:
    kinds = ["cli"]
    if row.get("application_artifact"):
        kinds.append("application")
    if row.get("headless_artifact"):
        kinds.append("headless")
    if row["os"] == "android" and row["target"].startswith("aarch64"):
        kinds += ["store-fdroid", "store-google-play", "store-appgallery"]
    for kind in kinds:
        item = dict(row, kind=kind, container="")
        if kind == "application":
            item["builder"] = row.get("application_builder", row["builder"])
        if row["os"] == "android" and (kind == "application" or kind.startswith("store-")):
            item["container"] = "android"
        elif row["os"] == "ohos":
            item["container"] = "ohos"
        elif row["target"].startswith("riscv64gc-") and item["builder"] != "cargo-alpine":
            item["container"] = "cross"
        matrix.append(item)
for arch, runner in [("x86_64", "ubuntu-latest"), ("aarch64", "ubuntu-24.04-arm")]:
    matrix.append(dict(target=f"{arch}-unknown-linux-musl", os="linux", runner=runner,
                       kind="serverless", builder="cargo-alpine", container=""))
matrix.append(dict(target="wasm32-unknown-unknown", os="wasm", runner="ubuntu-latest",
                   kind="edge", builder="cargo", container=""))
selected = [row for row in matrix if (args.target == "all" or row["target"] == args.target)
            and (args.kind == "all" or row["kind"] == args.kind)]
if not selected:
    parser.error("No build matches the requested target/product")
print(json.dumps({"include": selected}))
