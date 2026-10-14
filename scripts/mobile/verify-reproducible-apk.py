#!/usr/bin/env python3
"""Verify an independent rebuild by copying the trusted upstream APK signature."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

import apksigcopier

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("signed", type=Path)
parser.add_argument("rebuilt", type=Path)
parser.add_argument("--build-tools", type=Path, required=True, help="Android build-tools 34.0.0 directory")
parser.add_argument("--expected-cert-sha256", required=True)
parser.add_argument("--report", type=Path, required=True)
args = parser.parse_args()
expected = args.expected_cert_sha256.replace(":", "").lower()
if not re.fullmatch(r"[0-9a-f]{64}", expected):
    parser.error("Expected a SHA-256 certificate fingerprint")
properties = (args.build_tools / "source.properties").read_text()
if not re.search(r"^Pkg\.Revision\s*=\s*34\.0\.0\s*$", properties, re.MULTILINE):
    parser.error("Use the pinned Android build-tools 34.0.0")
verify = ("java", "-jar", str(args.build_tools / "lib/apksigner.jar"), "verify")
report = {"signed": str(args.signed), "rebuilt": str(args.rebuilt),
          "expected_certificate_sha256": expected, "passed": False}
try:
    details = subprocess.check_output([*verify, "--verbose", "--print-certs", str(args.signed)], text=True)
    certificates = re.findall(r"Signer #\d+ certificate SHA-256 digest: ([0-9a-fA-F]{64})", details)
    if [value.lower() for value in certificates] != [expected]:
        raise ValueError("Reference APK does not have the expected signing certificate")
    # Do not ignore ZIP metadata differences or compare only extracted files.
    apksigcopier.do_compare(str(args.signed), str(args.rebuilt), unsigned=True,
                           verify_cmd=verify, ignore_differences=False)
    for name, path in (("signed_sha256", args.signed), ("rebuilt_sha256", args.rebuilt)):
        with path.open("rb") as stream:
            report[name] = hashlib.file_digest(stream, "sha256").hexdigest()
    report["passed"] = True
    print("Upstream signature successfully verifies the rebuilt APK")
except Exception as error:
    report["error"] = str(error)
    raise
finally:
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
