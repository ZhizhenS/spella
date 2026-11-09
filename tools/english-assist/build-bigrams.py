#!/usr/bin/env python3
"""Compile the pinned offline bigram asset; normal builds never refresh its manifest.

RTENBG01 is little-endian: 32-byte header, 24-byte word records, 12-byte
edge records, a range-maximum edge-index tree, then the word string arena.
The companion manifest pins the complete image SHA256 and frozen input hashes.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

MAGIC = b"RTENBG01"
HEADER = struct.Struct("<8sIIII8s")
WORD = struct.Struct("<IIIIQ")
EDGE = struct.Struct("<IQ")
EMPTY = 0xFFFFFFFF
U64_MAX = (1 << 64) - 1
TERM = re.compile(r"[a-z]+(?:'[a-z]+)*", re.ASCII)
SOURCE_ROOT = Path("data/dict/raw/english-bigrams")
ASSET_ROOT = Path("core/english/assets")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_counts(data: bytes) -> dict[tuple[str, str], int]:
    pairs: dict[tuple[str, str], int] = {}
    for number, line in enumerate(data.decode("utf-8-sig").splitlines(), 1):
        if not line.strip():
            continue
        fields = line.split()
        if (len(fields) != 3 or not all(TERM.fullmatch(word) for word in fields[:2])
                or not fields[2].isascii() or not fields[2].isdigit()):
            raise ValueError(f"Invalid bigram row {number}")
        pair = (fields[0], fields[1])
        count = int(fields[2])
        if not 0 < count <= U64_MAX or pair in pairs:
            raise ValueError(f"Invalid count or duplicate bigram at row {number}")
        pairs[pair] = count
    if not pairs:
        raise ValueError("Bigram source is empty")
    return pairs


def maximum(left: int, right: int, counts: list[int]) -> int:
    if left == EMPTY:
        return right
    if right == EMPTY:
        return left
    return min(left, right) if counts[left] == counts[right] else (
        left if counts[left] > counts[right] else right)


def build_image(pairs: dict[tuple[str, str], int]) -> bytes:
    if not pairs:
        raise ValueError("Bigram source is empty")
    words = sorted({word for pair in pairs for word in pair})
    if any(not TERM.fullmatch(word) for word in words):
        raise ValueError("Bigram terms must be lowercase ASCII words")
    if len(words) >= EMPTY or len(pairs) >= EMPTY:
        raise ValueError("Bigram image exceeds the u32 index format")
    ids = {word: index for index, word in enumerate(words)}
    ordered = sorted(pairs.items())
    records = bytearray()
    edge_data = bytearray()
    arena = bytearray()
    counts: list[int] = []
    edge_index = 0
    for word in words:
        first = edge_index
        total = 0
        while edge_index < len(ordered) and ordered[edge_index][0][0] == word:
            (_, following), count = ordered[edge_index]
            if type(count) is not int or not 0 < count <= U64_MAX:
                raise ValueError("Bigram counts must be positive u64 values")
            total += count
            if total > U64_MAX:
                raise ValueError("Bigram row total exceeds u64")
            edge_data.extend(EDGE.pack(ids[following], count))
            counts.append(count)
            edge_index += 1
        encoded = word.encode("ascii")
        records.extend(WORD.pack(len(arena), len(encoded), first, edge_index - first, total))
        arena.extend(encoded)
    if len(arena) >= EMPTY:
        raise ValueError("Bigram string arena exceeds u32")
    tree_size = 1 << (len(pairs) - 1).bit_length()
    tree = [EMPTY] * (tree_size * 2)
    for index in range(len(pairs)):
        tree[tree_size + index] = index
    for index in range(tree_size - 1, 0, -1):
        tree[index] = maximum(tree[index * 2], tree[index * 2 + 1], counts)
    return b"".join((
        HEADER.pack(MAGIC, len(words), len(pairs), len(arena), tree_size, bytes(8)),
        records, edge_data, struct.pack(f"<{len(tree)}I", *tree), arena,
    ))


def validate_image(data: bytes) -> dict[str, int]:
    if len(data) < HEADER.size:
        raise ValueError("Truncated bigram header")
    magic, word_count, pair_count, arena_size, tree_size, reserved = HEADER.unpack_from(data)
    if (magic != MAGIC or reserved != bytes(8) or not word_count or not pair_count
            or tree_size != 1 << (pair_count - 1).bit_length()):
        raise ValueError("Invalid bigram header/version")
    edge_start = HEADER.size + word_count * WORD.size
    tree_start = edge_start + pair_count * EDGE.size
    arena_start = tree_start + 2 * tree_size * 4
    if arena_start + arena_size != len(data):
        raise ValueError("Invalid bigram image section lengths")
    counts: list[int] = []
    next_string = 0
    next_edge = 0
    previous_word = ""
    for index in range(word_count):
        offset, length, first, size, total = WORD.unpack_from(data, HEADER.size + index * WORD.size)
        if offset != next_string or not length or offset + length > arena_size:
            raise ValueError("Invalid bigram string record")
        word = data[arena_start + offset:arena_start + offset + length].decode("ascii")
        if not TERM.fullmatch(word) or word <= previous_word:
            raise ValueError("Bigram word records are not unique lexical terms")
        if first != next_edge or first + size > pair_count:
            raise ValueError("Invalid bigram adjacency range")
        actual_total = 0
        previous_id = -1
        for edge in range(first, first + size):
            next_id, count = EDGE.unpack_from(data, edge_start + edge * EDGE.size)
            if next_id <= previous_id or next_id >= word_count or not count:
                raise ValueError("Invalid or unsorted bigram edge")
            previous_id = next_id
            actual_total += count
            counts.append(count)
        if actual_total != total or actual_total > U64_MAX:
            raise ValueError("Invalid bigram row total")
        next_string += length
        next_edge += size
        previous_word = word
    if next_string != arena_size or next_edge != pair_count:
        raise ValueError("Unreferenced bigram strings or edges")
    tree = struct.unpack_from(f"<{tree_size * 2}I", data, tree_start)
    if tree[0] != EMPTY:
        raise ValueError("Invalid bigram tree sentinel")
    for index in range(tree_size):
        expected = index if index < pair_count else EMPTY
        if tree[tree_size + index] != expected:
            raise ValueError("Invalid bigram tree leaf")
    for index in range(tree_size - 1, 0, -1):
        if tree[index] != maximum(tree[index * 2], tree[index * 2 + 1], counts):
            raise ValueError("Invalid bigram range-maximum tree")
    return {"words": word_count, "pairs": pair_count, "arena_bytes": arena_size, "tree_size": tree_size}


def checked_file(root: Path, name: str, record: dict) -> Path:
    if not isinstance(name, str) or Path(name).name != name or "\\" in name:
        raise ValueError("Frozen filename must be a single basename")
    path = root / name
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"Missing or linked frozen file: {name}")
    data = path.read_bytes()
    if len(data) != record["bytes"] or sha256(data) != record["sha256"]:
        raise ValueError(f"Frozen source/license hash mismatch: {name}")
    return path


def prepare(repo: Path, *, refresh: bool = False, verify_only: bool = False) -> dict:
    if refresh and verify_only:
        raise ValueError("Cannot refresh in verify-only mode")
    root = repo / SOURCE_ROOT
    source_manifest_data = (root / "SOURCES.json").read_bytes()
    source = json.loads(source_manifest_data.decode("utf-8-sig"))
    if source.get("schema") != 1:
        raise ValueError("Unexpected frozen bigram source schema")
    record = source["source"]
    compressed = checked_file(root, record["file"], record)
    for name, license_record in source["licenses"].items():
        checked_file(root, name, license_record)
    with gzip.open(compressed, "rb") as stream:
        raw = stream.read(record["uncompressedBytes"] + 1)
    if len(raw) != record["uncompressedBytes"] or sha256(raw) != record["uncompressedSha256"]:
        raise ValueError("Frozen bigram uncompressed size/hash mismatch")
    pairs = parse_counts(raw)
    if len(pairs) != record["rows"]:
        raise ValueError("Frozen bigram row count mismatch")
    image = build_image(pairs)
    counts = validate_image(image)
    compiler = repo / "tools/english-assist/build-bigrams.py"
    manifest = {
        "schema": 1, "format": MAGIC.decode("ascii"), "file": "bigrams.bin",
        "bytes": len(image), "sha256": sha256(image), "counts": counts,
        "source": {key: record[key] for key in ("repository", "commit", "path", "sha256", "uncompressedSha256")},
        "source_manifest_sha256": sha256(source_manifest_data),
        "compiler_sha256": sha256(compiler.read_bytes()),
    }
    asset_root = repo / ASSET_ROOT
    expected_path = asset_root / "bigrams.json"
    output = asset_root / manifest["file"]
    if not refresh:
        expected = json.loads(expected_path.read_text(encoding="utf-8-sig"))
        if manifest != expected:
            raise ValueError("Bigram compiler/input differs from reviewed manifest; review before explicit --refresh-manifest")
    if verify_only:
        checked_file(asset_root, manifest["file"], manifest)
    else:
        asset_root.mkdir(parents=True, exist_ok=True)
        output.write_bytes(image)
        if refresh:
            expected_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--refresh-manifest", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    try:
        manifest = prepare(Path(__file__).resolve().parents[2], refresh=args.refresh_manifest,
                           verify_only=args.verify_only)
    except (OSError, ValueError, KeyError, struct.error) as error:
        raise SystemExit(f"Frozen bigram preparation failed: {error}") from error
    print(json.dumps(manifest, sort_keys=True))


if __name__ == "__main__":
    main()
