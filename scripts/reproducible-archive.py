#!/usr/bin/env python3
"""Create release ZIPs without checkout timestamps or filesystem enumeration order."""
import argparse
from datetime import datetime, timezone
import fnmatch
import os
from pathlib import Path
import stat
import subprocess
import zipfile


def source_epoch():
    value = os.environ.get("SOURCE_DATE_EPOCH")
    if value is None:
        value = subprocess.check_output(
            ["git", "show", "-s", "--format=%ct", "HEAD"],
            cwd=Path(__file__).resolve().parents[1], text=True).strip()
    return int(value)


def archive(root, output, paths, excludes=()):
    root = root.resolve()
    entries = {}

    def collect(path):
        name = path.relative_to(root).as_posix()
        if any(fnmatch.fnmatchcase(name, pattern) for pattern in excludes):
            return
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            entries[name] = (path, stat.S_IFLNK | 0o777)
        elif stat.S_ISDIR(mode):
            if name != ".":
                entries[name + "/"] = (path, stat.S_IFDIR | 0o755)
            for child in path.iterdir():
                collect(child)
        elif stat.S_ISREG(mode):
            entries[name] = (path, stat.S_IFREG | (0o755 if mode & 0o111 else 0o644))
        else:
            raise ValueError(f"Unsupported release archive entry: {path}")

    for name in paths:
        path = root / name
        if Path(name).is_absolute() or ".." in Path(name).parts:
            raise ValueError(f"Archive path must be relative to root: {name}")
        collect(path)
    # ZIP has a 1980 epoch and two-second precision. Always encode UTC.
    timestamp = datetime.fromtimestamp(max(315532800, source_epoch()), timezone.utc)
    date = timestamp.timetuple()[:6]
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as result:
        for name, (path, mode) in sorted(entries.items()):
            info = zipfile.ZipInfo(name, date)
            info.create_system = 3
            info.external_attr = mode << 16
            if stat.S_ISDIR(mode):
                info.external_attr |= 0x10
                data = b""
            elif stat.S_ISLNK(mode):
                data = os.fsencode(os.readlink(path))
            else:
                data = path.read_bytes()
            result.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--exclude", action="append", default=[])
    parser.add_argument("paths", nargs="*", default=["."])
    args = parser.parse_args()
    archive(args.root, args.output, args.paths, args.exclude)
