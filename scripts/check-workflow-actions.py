#!/usr/bin/env python3
"""Reject mutable external action references in workflows and composite actions."""

from pathlib import Path
import re
import sys


def immutable(ref: str) -> bool:
    if ref.startswith("./"):
        return True
    if ref.startswith("docker://"):
        return re.fullmatch(r"docker://[^\s@]+@sha256:[0-9a-f]{64}", ref) is not None
    return re.fullmatch(r"[\w.-]+/[\w./-]+@[0-9a-f]{40}", ref) is not None


def main() -> int:
    errors = []
    for path in sorted(Path(".github").rglob("*")):
        if path.suffix not in {".yml", ".yaml"}:
            continue
        for number, line in enumerate(path.read_text().splitlines(), 1):
            match = re.match(r"\s*(?:-\s*)?uses:\s*([^#]+)", line)
            if match and not immutable(match[1].strip().strip("\"'")):
                errors.append(f"{path}:{number}: pin external actions to full commit SHAs")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("All external action references are immutable.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
