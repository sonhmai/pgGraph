#!/usr/bin/env python3
"""Inspect a `.pggraph` artifact and print section-size JSON.

Usage:
  python3 scripts/inspect_pggraph_artifact.py /path/to/main.pggraph

The checker validates the file header, format version, monotonic section
offsets, section bounds, and CRC32. It does not require PostgreSQL, pgrx, git,
or a running database.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import struct
import sys
import zlib

MAGIC = b"PGGH"
VERSION = 1
HEADER_SIZE = 128
SECTION_NAMES = [
    "is_active",
    "table_oids",
    "edge_offsets",
    "targets",
    "type_ids",
    "weights",
    "resolution_index",
    "primary_key_offsets",
    "primary_key_bytes",
    "filter_index",
    "edge_type_registry",
]
CRC_OFFSET = 20 + len(SECTION_NAMES) * 8


def inspect(path: pathlib.Path) -> dict[str, object]:
    data = path.read_bytes()
    if len(data) < HEADER_SIZE:
        raise ValueError(f"{path} is too small to be a .pggraph artifact")
    if data[:4] != MAGIC:
        raise ValueError(f"{path} has invalid magic bytes")

    version, flags, node_count, edge_count = struct.unpack_from("<IIII", data, 4)
    if version != VERSION:
        raise ValueError(f"{path} has version {version}, expected {VERSION}")

    offsets = list(struct.unpack_from(f"<{len(SECTION_NAMES)}Q", data, 20))
    if offsets != sorted(offsets):
        raise ValueError("section offsets are not monotonic")
    if offsets[0] < HEADER_SIZE or offsets[-1] > len(data):
        raise ValueError("section offsets are outside the artifact")

    ends = offsets[1:] + [len(data)]
    sections = [
        {
            "name": name,
            "offset": start,
            "size_bytes": end - start,
        }
        for name, start, end in zip(SECTION_NAMES, offsets, ends)
    ]

    expected_crc = struct.unpack_from("<I", data, CRC_OFFSET)[0]
    actual_crc = zlib.crc32(data[HEADER_SIZE:]) & 0xFFFFFFFF

    return {
        "path": str(path),
        "file_size_bytes": len(data),
        "version": version,
        "flags": flags,
        "node_count": node_count,
        "edge_count": edge_count,
        "section_count": len(SECTION_NAMES),
        "crc32_valid": expected_crc == actual_crc,
        "sections": sections,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=pathlib.Path)
    args = parser.parse_args()

    try:
        report = inspect(args.artifact)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
