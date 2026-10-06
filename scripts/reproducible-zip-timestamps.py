#!/usr/bin/env python3
"""Set unsigned ZIP timestamps without repacking data or changing alignment."""
import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import struct
import zipfile


def normalize(path, epoch):
    date = datetime.fromtimestamp(max(epoch, 315532800), timezone.utc)
    dos_time = (date.hour << 11) | (date.minute << 5) | (date.second // 2)
    dos_date = ((date.year - 1980) << 9) | (date.month << 5) | date.day
    stamp = struct.pack("<HH", dos_time, dos_date)
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        central = archive.start_dir
    with path.open("r+b") as stream:
        for entry in entries:
            stream.seek(entry.header_offset)
            if stream.read(4) != b"PK\x03\x04":
                raise ValueError("Invalid local ZIP header")
            stream.seek(entry.header_offset + 10)
            stream.write(stamp)
            stream.seek(central)
            header = stream.read(46)
            if header[:4] != b"PK\x01\x02":
                raise ValueError("Invalid central ZIP header")
            name, extra, comment = struct.unpack_from("<HHH", header, 28)
            stream.seek(central + 12)
            stream.write(stamp)
            central += 46 + name + extra + comment
    with zipfile.ZipFile(path) as archive:
        if archive.testzip() is not None:
            raise ValueError("Invalid ZIP content checksum")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    args = parser.parse_args()
    normalize(args.archive, int(os.environ["SOURCE_DATE_EPOCH"]))
