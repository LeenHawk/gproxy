#!/usr/bin/env python3
"""Verify ad-hoc Mach-O code/resource hashes without requiring a CMS certificate.

The release targets are thin little-endian 64-bit Mach-O executables. This is
an integrity check, not Apple certificate validation or notarization.
"""
import hashlib
from pathlib import Path
import plistlib
import struct
import sys


def verify(path):
    path = Path(path)
    data = path.read_bytes()
    if struct.unpack_from("<I", data)[0] != 0xFEEDFACF:
        raise ValueError("Expected a thin 64-bit Mach-O executable")
    command = 32
    signature = None
    for _ in range(struct.unpack_from("<I", data, 16)[0]):
        kind, size = struct.unpack_from("<II", data, command)
        if kind == 0x1D:  # LC_CODE_SIGNATURE
            offset, length = struct.unpack_from("<II", data, command + 8)
            signature = data[offset:offset + length]
            code_end = offset
        if size < 8:
            raise ValueError("Invalid Mach-O load command")
        command += size
    if signature is None:
        raise ValueError("Mach-O has no code signature")
    magic, length, count = struct.unpack_from(">III", signature)
    if magic != 0xFADE0CC0 or length > len(signature):
        raise ValueError("Invalid code signature superblob")
    blobs = {}
    for index in range(count):
        slot, offset = struct.unpack_from(">II", signature, 12 + index * 8)
        size = struct.unpack_from(">I", signature, offset + 4)[0]
        blobs[slot] = signature[offset:offset + size]
        if len(blobs[slot]) != size:
            raise ValueError("Truncated code signature blob")
    contents = path.parent.parent
    external = {}
    if contents.name == "Contents":
        external = {1: (contents / "Info.plist").read_bytes(),
                    3: (contents / "_CodeSignature/CodeResources").read_bytes()}
    directories = [blob for blob in blobs.values() if struct.unpack_from(">I", blob)[0] == 0xFADE0C02]
    if not directories:
        raise ValueError("Code signature has no CodeDirectory")
    for directory in directories:
        _, size, version, flags, hashes, _, special, slots, limit = struct.unpack_from(">9I", directory)
        hash_size, hash_type, _, page_exponent = struct.unpack_from("4B", directory, 36)
        if not flags & 2:  # CS_ADHOC
            raise ValueError("Expected an ad-hoc signature, not a certificate signature")
        if size != len(directory):
            raise ValueError("Invalid CodeDirectory length")
        if version >= 0x20100 and struct.unpack_from(">I", directory, 44)[0]:
            raise ValueError("Scatter CodeDirectories are not supported")
        if version >= 0x20300 and limit == 0xFFFFFFFF:
            limit = struct.unpack_from(">Q", directory, 56)[0]
        if limit != code_end:
            raise ValueError("CodeDirectory does not cover the whole executable")
        algorithm, digest_size = {1: ("sha1", 20), 2: ("sha256", 32),
                                  3: ("sha256", 20), 4: ("sha384", 48)}[hash_type]
        if hash_size != digest_size:
            raise ValueError("Invalid CodeDirectory digest size")
        page = 1 << page_exponent if page_exponent else limit
        if slots != (limit + page - 1) // page:
            raise ValueError("Incorrect CodeDirectory page count")
        for index in range(slots):
            actual = hashlib.new(algorithm, data[index * page:min((index + 1) * page, limit)]).digest()[:hash_size]
            expected = directory[hashes + index * hash_size:hashes + (index + 1) * hash_size]
            if actual != expected:
                raise ValueError(f"Code signature page {index} does not match")
        for slot in range(1, special + 1):
            expected = directory[hashes - slot * hash_size:hashes - (slot - 1) * hash_size]
            if expected == bytes(hash_size):
                continue
            blob = external.get(slot, blobs.get(slot))
            if blob is None or hashlib.new(algorithm, blob).digest()[:hash_size] != expected:
                raise ValueError(f"Code signature special slot {slot} does not match")
    if external:
        resources = plistlib.loads(external[3])
        for name, record in resources.get("files2", {}).items():
            if isinstance(record, dict) and "hash2" in record:
                if hashlib.sha256((contents / name).read_bytes()).digest() != record["hash2"]:
                    raise ValueError(f"Application resource does not match: {name}")
    print(f"Verified ad-hoc code and resource hashes: {path.name}")


if __name__ == "__main__":
    verify(sys.argv[1])
