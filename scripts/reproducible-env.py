#!/usr/bin/env python3
"""Set source-derived build identity and deterministic tool defaults."""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def environment():
    epoch = os.environ.get("SOURCE_DATE_EPOCH") or subprocess.check_output(
        ["git", "show", "-s", "--format=%ct", "HEAD"], cwd=ROOT, text=True).strip()
    if int(epoch) < 0:
        raise ValueError("SOURCE_DATE_EPOCH must be nonnegative")
    commit = os.environ.get("GPROXY_BUILD_HASH") or subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    return {"SOURCE_DATE_EPOCH": epoch, "TZ": "UTC", "ZERO_AR_DATE": "1",
            "GPROXY_BUILD_HASH": commit}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=["shell", "github", "json"], default="shell")
    parser.add_argument("--normalize-tree", type=Path)
    args = parser.parse_args()
    values = environment()
    if args.normalize_tree:
        # Work only on staging directories, never on the source or Cargo cache.
        epoch = int(values["SOURCE_DATE_EPOCH"])
        paths = [args.normalize_tree, *args.normalize_tree.rglob("*")]
        for path in paths:
            if path.is_symlink():
                os.utime(path, (epoch, epoch), follow_symlinks=False)
            else:
                os.utime(path, (epoch, epoch))
    elif args.format == "json":
        print(json.dumps(values))
    elif args.format == "github":
        with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as stream:
            for key, value in values.items():
                stream.write(f"{key}={value}\n")
    else:
        for key, value in values.items():
            print(f"export {key}={shlex.quote(value)}")
