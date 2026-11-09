#!/usr/bin/env python3
"""Pure compiler/format/provenance fixtures; no native build or system changes."""
from __future__ import annotations

import bisect
import gzip
import importlib.util
import itertools
import json
from pathlib import Path
import shutil
import struct
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("build-bigrams.py")
spec = importlib.util.spec_from_file_location("build_bigrams", SCRIPT)
compiler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compiler)


def decoded(data: bytes):
    _, count, pairs, _, tree_size, _ = compiler.HEADER.unpack_from(data)
    edge_start = 32 + count * 24
    tree_start = edge_start + pairs * 12
    arena_start = tree_start + tree_size * 8
    words, records = [], []
    for index in range(count):
        record = compiler.WORD.unpack_from(data, 32 + index * 24)
        offset, size = record[:2]
        words.append(data[arena_start + offset:arena_start + offset + size].decode("ascii"))
        records.append(record)
    edges = [compiler.EDGE.unpack_from(data, edge_start + index * 12) for index in range(pairs)]
    tree = struct.unpack_from(f"<{tree_size * 2}I", data, tree_start)
    return words, records, edges, tree, tree_size


def range_max(tree, size, left, right, edges):
    # Consumer-side segment-tree query, checked against a brute-force oracle.
    nodes = []
    left += size
    right += size
    while left < right:
        if left & 1:
            nodes.append(tree[left])
            left += 1
        if right & 1:
            right -= 1
            nodes.append(tree[right])
        left //= 2
        right //= 2
    valid = [index for index in nodes if index != compiler.EMPTY]
    return min(valid, key=lambda index: (-edges[index][1], index)) if valid else compiler.EMPTY


class CompilerTests(unittest.TestCase):
    def test_row_order_does_not_change_image(self):
        rows = ["can you 100", "can your 30", "you are 90", "you can 90"]
        expected = compiler.build_image(compiler.parse_counts("\n".join(rows).encode()))
        for order in itertools.permutations(rows):
            self.assertEqual(compiler.build_image(compiler.parse_counts("\n".join(order).encode())), expected)

    def test_contract_offsets_totals_and_terminal_only_words(self):
        image = compiler.build_image({("can", "you"): 100, ("can", "your"): 30, ("you", "are"): 90})
        self.assertEqual(compiler.validate_image(image), {"words": 4, "pairs": 3, "arena_bytes": 13, "tree_size": 4})
        words, records, edges, tree, _ = decoded(image)
        self.assertEqual(words, ["are", "can", "you", "your"])
        self.assertEqual(records[0][2:], (0, 0, 0))
        self.assertEqual(records[1][2:], (0, 2, 130))
        self.assertEqual(edges, [(2, 100), (3, 30), (0, 90)])
        self.assertEqual(tree[0], compiler.EMPTY)
        self.assertEqual(tree[1], 0)
        self.assertEqual(tree[-1], compiler.EMPTY)

    def test_every_range_max_matches_oracle_with_ties(self):
        pairs = {("we", word): count for word, count in zip(
            ["are", "can", "do", "have", "like", "will", "would"], [5, 90, 90, 4, 80, 90, 1])}
        words, records, edges, tree, size = decoded(compiler.build_image(pairs))
        for left in range(len(edges) + 1):
            for right in range(left, len(edges) + 1):
                oracle = min(range(left, right), key=lambda index: (-edges[index][1], index), default=compiler.EMPTY)
                self.assertEqual(range_max(tree, size, left, right, edges), oracle)
        row = records[words.index("we")]
        low, high = bisect.bisect_left(words, "w"), bisect.bisect_left(words, "w{")
        candidates = [index for index in range(row[2], row[2] + row[3]) if low <= edges[index][0] < high]
        self.assertEqual([words[edges[index][0]] for index in candidates], ["will", "would"])

    def test_invalid_source_is_rejected(self):
        for raw in [b"", b"one two", b"one two 0", b"one two -1", b"one two 1.0",
                    b"One two 1", b"one two 1 extra", b"one two 1\none two 2",
                    b"one two 18446744073709551616", b"'one two 1", b"one two' 1",
                    "one two １".encode(), "oné two 1".encode()]:
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                compiler.parse_counts(raw)

    def test_internal_apostrophes_and_u64_count_roundtrip(self):
        image = compiler.build_image(compiler.parse_counts(b"don't stop 177045273024\n"))
        self.assertEqual(compiler.validate_image(image)["pairs"], 1)
        self.assertEqual(decoded(image)[2][0][1], 177045273024)

    def test_row_total_overflow_is_rejected(self):
        with self.assertRaises(ValueError):
            compiler.build_image({("can", "you"): compiler.U64_MAX, ("can", "your"): 1})

    def test_malformed_binary_sections_are_rejected(self):
        image = compiler.build_image({("can", "you"): 10, ("you", "can"): 20})
        variants = [image[:31], image[:-1], image + b"x"]
        for offset, value in [(0, 0), (24, 1), (32, 1), (32 + 16, 1),
                              (32 + 2 * 24, 255), (32 + 2 * 24 + 4, 0),
                              (32 + 2 * 24 + 2 * 12, 0),
                              (32 + 2 * 24 + 2 * 12 + 4, 0)]:
            changed = bytearray(image)
            changed[offset] = value
            variants.append(bytes(changed))
        for index, variant in enumerate(variants):
            with self.subTest(index=index), self.assertRaises((ValueError, UnicodeError)):
                compiler.validate_image(variant)


class FrozenTests(unittest.TestCase):
    def fixture(self, repo: Path):
        root = repo / compiler.SOURCE_ROOT
        root.mkdir(parents=True)
        raw = b"can you 100\nyou are 90\n"
        compressed = gzip.compress(raw, mtime=0)
        (root / "frequency-bigrams.txt.gz").write_bytes(compressed)
        license_data = b"MIT fixture license\n"
        (root / "LICENSE-SymSpell").write_bytes(license_data)
        source = {"schema": 1, "source": {
            "repository": "https://github.com/wolfgarbe/SymSpell", "commit": "fixture",
            "path": "fixture.txt", "file": "frequency-bigrams.txt.gz", "bytes": len(compressed),
            "sha256": compiler.sha256(compressed), "uncompressedBytes": len(raw),
            "uncompressedSha256": compiler.sha256(raw), "rows": 2,
        }, "licenses": {"LICENSE-SymSpell": {"bytes": len(license_data), "sha256": compiler.sha256(license_data)}}}
        (root / "SOURCES.json").write_text(json.dumps(source), encoding="utf-8")
        script = repo / "tools/english-assist/build-bigrams.py"
        script.parent.mkdir(parents=True)
        shutil.copyfile(SCRIPT, script)
        return root

    def test_repeat_build_and_verify_only_leave_reviewed_manifest_unchanged(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            self.fixture(repo)
            expected = compiler.prepare(repo, refresh=True)
            manifest = repo / compiler.ASSET_ROOT / "bigrams.json"
            before = manifest.read_bytes()
            self.assertEqual(compiler.prepare(repo), expected)
            self.assertEqual(compiler.prepare(repo, verify_only=True), expected)
            self.assertEqual(manifest.read_bytes(), before)
            with self.assertRaises(ValueError):
                compiler.prepare(repo, refresh=True, verify_only=True)

    def test_source_license_or_asset_mutation_fails_verification(self):
        for target in ["source", "license", "asset"]:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temp:
                repo = Path(temp)
                root = self.fixture(repo)
                compiler.prepare(repo, refresh=True)
                path = {"source": root / "frequency-bigrams.txt.gz", "license": root / "LICENSE-SymSpell",
                        "asset": repo / compiler.ASSET_ROOT / "bigrams.bin"}[target]
                path.write_bytes(path.read_bytes() + b"corrupt")
                with self.assertRaises(ValueError):
                    compiler.prepare(repo, verify_only=True)

    def test_compiler_change_cannot_silently_refresh_manifest(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            self.fixture(repo)
            compiler.prepare(repo, refresh=True)
            manifest = repo / compiler.ASSET_ROOT / "bigrams.json"
            before = manifest.read_bytes()
            (repo / "tools/english-assist/build-bigrams.py").write_bytes(b"different compiler")
            with self.assertRaises(ValueError):
                compiler.prepare(repo)
            self.assertEqual(manifest.read_bytes(), before)

    def test_source_escape_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            for name in ["../outside", "..\\outside", "/absolute"]:
                with self.subTest(name=name), self.assertRaises(ValueError):
                    compiler.checked_file(Path(temp), name, {})

    def test_build_hook_precedes_first_cargo_compile(self):
        repo = SCRIPT.resolve().parents[2]
        build = (repo / "platforms/windows/installer/build-english-assist.ps1").read_text(encoding="utf-8-sig")
        hook = build.index("build-english-bigrams.ps1")
        self.assertLess(hook, build.index("& cargo build"))
        self.assertNotIn("--refresh-manifest", build)

    def test_git_checkout_preserves_reviewed_input_and_image_hashes(self):
        repo = SCRIPT.resolve().parents[2]
        attributes = (repo / ".gitattributes").read_text(encoding="utf-8")
        policies = {}
        for line in attributes.splitlines():
            parts = line.split()
            if len(parts) > 1 and parts[0].startswith("/"):
                policies[parts[0][1:]] = parts[1:]
        manifest = json.loads((repo / compiler.ASSET_ROOT / "bigrams.json").read_bytes())
        source = json.loads((repo / compiler.SOURCE_ROOT / "SOURCES.json").read_bytes())
        pinned = {
            "tools/english-assist/build-bigrams.py": manifest["compiler_sha256"],
            "core/english/assets/bigrams.bin": manifest["sha256"],
            "data/dict/raw/english-bigrams/SOURCES.json": manifest["source_manifest_sha256"],
            "data/dict/raw/english-bigrams/frequency-bigrams.txt.gz": source["source"]["sha256"],
            "data/dict/raw/english-bigrams/LICENSE-SymSpell": source["licenses"]["LICENSE-SymSpell"]["sha256"],
        }
        for path, expected in pinned.items():
            with self.subTest(path=path):
                policy = policies.get(path, [])
                data = (repo / path).read_bytes()
                if "-text" not in policy:
                    self.assertIn("text", policy)
                    self.assertIn("eol=lf", policy)
                    data = data.replace(b"\r\n", b"\n")
                self.assertEqual(compiler.sha256(data), expected,
                                 "Git checkout normalization would break the frozen receipt")

    def test_real_pinned_image_and_known_pairs(self):
        repo = SCRIPT.resolve().parents[2]
        manifest = compiler.prepare(repo, verify_only=True)
        self.assertEqual(manifest["counts"]["pairs"], 242342)
        words, records, edges, _, _ = decoded((repo / compiler.ASSET_ROOT / "bigrams.bin").read_bytes())
        for previous, following in [("can", "you"), ("you", "are"), ("a", "lot")]:
            row = records[words.index(previous)]
            observed = {words[next_id]: count for next_id, count in edges[row[2]:row[2] + row[3]]}
            self.assertGreater(observed[following], 0)


if __name__ == "__main__":
    unittest.main()
