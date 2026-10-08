#!/usr/bin/env python3
"""Build a deterministic HFS+ volume, then wrap it in a verified UDZO image."""
import argparse
from datetime import datetime, timezone
import hashlib
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile


def tree_entries(root):
    entries = {}
    for path in sorted(root.rglob("*")):
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            value = b"link\0" + os.fsencode(os.readlink(path))
        elif path.is_dir():
            value = b"directory\0"
        else:
            value = b"executable\0" if path.stat().st_mode & 0o111 else b"file\0"
            with path.open("rb") as stream:
                value += hashlib.file_digest(stream, "sha256").digest()
        entries[name] = value
    return entries


def tree_hash(root):
    digest = hashlib.sha256()
    for name, value in tree_entries(root).items():
        digest.update(name.encode() + b"\0")
        digest.update(value)
    return digest.digest()


def hfs_partition(image):
    if image[:2] != b"ER":
        raise ValueError("Expected an Apple partition map")
    block = struct.unpack_from(">H", image, 2)[0]
    count = struct.unpack_from(">I", image, block + 4)[0]
    partitions = []
    for number in range(1, count + 1):
        header = image[number * block:(number + 1) * block]
        if header[:2] != b"PM":
            raise ValueError("Invalid Apple partition entry")
        if header[48:80].split(b"\0")[0] == b"Apple_HFS":
            start, size = struct.unpack_from(">II", header, 8)
            volume = image[start * block:(start + size) * block]
            if len(volume) != size * block or volume[1024:1026] != b"H+":
                raise ValueError("Invalid HFS+ volume")
            partitions.append(volume)
    if len(partitions) != 1:
        raise ValueError("Expected exactly one HFS+ volume")
    return partitions[0]


def package(source, output, volume_name):
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH") or subprocess.check_output(
        ["git", "show", "-s", "--format=%ct", "HEAD"],
        cwd=Path(__file__).resolve().parents[1], text=True).strip())
    date = datetime.fromtimestamp(epoch, timezone.utc).strftime("%Y%m%d%H%M%S") + "00"
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="gproxy-dmg-") as directory:
        root = Path(directory)
        payload = root / "payload"
        shutil.copytree(source, payload, symlinks=True)
        for path in [payload, *payload.rglob("*")]:
            if not path.is_symlink():
                executable = path.is_dir() or path.stat().st_mode & 0o111
                path.chmod(0o755 if executable else 0o644)
        identity = tree_hash(payload)
        image = root / "hybrid.iso"
        subprocess.run([
            "xorriso", "-as", "mkisofs", "-quiet", "-R", "-uid", "0", "-gid", "0",
            "-hfsplus", "-apm-block-size", "512", "-hfsplus-serial-no", identity[:8].hex(),
            f"--modification-date={date}", "--set_all_file_dates", date,
            "-V", volume_name, "-o", str(image), str(payload),
        ], env={**os.environ, "SOURCE_DATE_EPOCH": str(epoch)}, check=True)
        raw = root / "volume.hfs"
        raw.write_bytes(hfs_partition(image.read_bytes()))
        subprocess.run(["hdiutil", "convert", str(raw), "-format", "UDZO",
                        "-srcimagekey", "diskimage-class=CRawDiskImage",
                        "-tgtimagekey", "zlib-level=9", "-ov", "-o", str(output)], check=True)
        # A single-segment UDIF uses this identifier only to associate segments.
        # It is outside the data-fork/master checksums. Set it before any signing.
        with output.open("r+b") as stream:
            stream.seek(-512, os.SEEK_END)
            footer = stream.read(512)
            if footer[:4] != b"koly" or struct.unpack_from(">II", footer, 4) != (4, 512):
                raise ValueError("Unsupported UDIF footer")
            if struct.unpack_from(">I", footer, 60)[0] != 1:
                raise ValueError("Only single-segment unsigned DMGs are supported")
            stream.seek(-512 + 64, os.SEEK_END)
            stream.write(hashlib.sha256(raw.read_bytes()).digest()[:16])
        subprocess.run(["hdiutil", "verify", str(output)], check=True)
        mount = root / "mount"
        mount.mkdir()
        subprocess.run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", str(mount), str(output)], check=True)
        try:
            if tree_hash(mount) != identity:
                expected, actual = tree_entries(payload), tree_entries(mount)
                differences = [
                    f"{name!r}: staged={expected.get(name)!r}, mounted={actual.get(name)!r}"
                    for name in sorted(expected.keys() | actual.keys())
                    if expected.get(name) != actual.get(name)
                ]
                raise ValueError("Mounted DMG contents differ from the staged payload:\n"
                                 + "\n".join(differences))
            for app in mount.glob("*.app"):
                if (app / "Contents/_CodeSignature/CodeResources").is_file():
                    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
        finally:
            subprocess.run(["hdiutil", "detach", str(mount)], check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--volume-name", required=True)
    args = parser.parse_args()
    package(args.source, args.output, args.volume_name)
