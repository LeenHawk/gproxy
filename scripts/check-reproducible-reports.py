#!/usr/bin/env python3
"""Require a successful report for every requested target/product."""
import importlib.util
import json
import os
from pathlib import Path
import sys

root = Path(sys.argv[1])
matrix = json.loads(os.environ["REPRO_MATRIX"])["include"]
if os.environ.get("REPRO_FULL_MATRIX") == "true":
    matrix += [dict(row, kind="container") for row in json.loads(os.environ["REPRO_CONTAINER_MATRIX"])["include"]]
commit = os.environ["GITHUB_SHA"]
failures = []
packages = set()
for row in matrix:
    name = f"repro-report-{row['kind']}-{row['target']}"
    path = root / name / "report.json"
    if not path.is_file():
        failures.append(f"{name}: missing report")
        continue
    report = json.loads(path.read_text())
    if report.get("commit") != commit or report.get("passed") is not True:
        failures.append(f"{name}: build/comparison failed or source mismatch")
    packages.update(entry["name"] for entry in report.get("files", []) if entry["equal"])
if os.environ.get("REPRO_FULL_MATRIX") == "true":
    spec = importlib.util.spec_from_file_location("verify", Path(__file__).with_name("verify-reproducible.py"))
    verify = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verify)
    for name in verify.expected_packages():
        if name not in packages:
            failures.append(f"{name}: no matching pair of release packages")
summary = {"commit": commit, "requested_builds": len(matrix), "passed": not failures, "failures": failures}
Path("dist/reproducible-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
for failure in failures:
    print(failure)
if not failures:
    print(f"All {len(matrix)} requested target/product comparisons passed")
sys.exit(bool(failures))
