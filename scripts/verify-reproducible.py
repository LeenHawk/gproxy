#!/usr/bin/env python3
"""Compare complete release package sets byte-for-byte; missing targets fail."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def expected_packages():
    rows = json.loads((ROOT / "scripts/release-targets.json").read_text())["include"]
    names = {"gproxy-edge.wasm", "gproxy-edge-cloudflare.zip"}
    for arch in ("x86_64", "aarch64"):
        names.add(f"gproxy-serverless-linux-{arch}-musl.zip")
    cli_extensions = {"linux": ".deb", "android": ".deb", "windows": ".msix", "macos": ".dmg"}
    app_extensions = {"linux": ".deb", "android": ".apk", "windows": ".msix", "macos": ".dmg", "ohos": ".hap"}
    for row in rows:
        cli = row["artifact"]
        names.add(cli + ".zip")
        if row["os"] in cli_extensions:
            names.add(cli + cli_extensions[row["os"]])
        if row["target"].endswith("-linux-musl"):
            names.add(cli + ".apk")
        if headless := row.get("headless_artifact"):
            names.add(headless + ".zip")
            if row["os"] == "android":
                names.add(headless + ".deb")
            if row["target"].endswith("-linux-musl"):
                names.add(headless + ".apk")
        if app := row.get("application_artifact"):
            names.add(app + row.get("application_extension", app_extensions[row["os"]]))
            for extension in row.get("application_extra_extensions", []):
                names.add(app + extension)
            if row["os"] in ("linux", "windows", "macos"):
                names.add(app + ".zip")
    return sorted(names)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def compare(first, second, names):
    results = []
    for name in names:
        left, right = first / name, second / name
        result = {"name": name, "first": None, "second": None}
        if left.is_file():
            result["first"] = digest(left)
        if right.is_file():
            result["second"] = digest(right)
        result["status"] = ("missing" if None in (result["first"], result["second"])
                            else "equal" if result["first"] == result["second"] else "different")
        results.append(result)
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("first", type=Path)
    parser.add_argument("second", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    results = compare(args.first, args.second, expected_packages())
    report = {"scope": "release packages (containers and store-only builds require separate comparison)",
              "passed": all(row["status"] == "equal" for row in results), "packages": results}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    for row in results:
        print(f"{row['status']:9} {row['name']}")
    sys.exit(0 if report["passed"] else 1)
