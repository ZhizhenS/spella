"""Integrity policy tests; write only to temporary directories, never installed resources."""
import copy
import gzip
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("prepare_lexicon", Path(__file__).with_name("prepare-lexicon.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class IntegrityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.raw = b"frozen dictionary\n"
        self.packed = gzip.compress(self.raw, mtime=0)
        (self.root / "source.txt.gz").write_bytes(self.packed)
        self.source = {"file": "source.txt.gz", "bytes": len(self.packed),
                       "sha256": hashlib.sha256(self.packed).hexdigest(),
                       "uncompressedBytes": len(self.raw), "uncompressedSha256": hashlib.sha256(self.raw).hexdigest()}
        self.binary = b"RTENLX02test-data"
        (self.root / "english-lexicon-v2.bin").write_bytes(self.binary)
        self.manifest = {"schema": 2, "format": "RTENLX02", "file": "english-lexicon-v2.bin",
                         "bytes": len(self.binary), "sha256": hashlib.sha256(self.binary).hexdigest(),
                         "counts": {"words": 3, "pinyin": 1, "mappings": 2}}

    def test_frozen_source_accepted(self):
        self.assertEqual(module.verify_source(self.root, self.source).name, "source.txt.gz")

    def test_changed_archive_rejected(self):
        (self.root / self.source["file"]).write_bytes(self.packed[:-1] + b"x")
        with self.assertRaisesRegex(ValueError, "size/hash"):
            module.verify_source(self.root, self.source)

    def test_expansion_beyond_frozen_size_rejected(self):
        self.source["uncompressedBytes"] -= 1
        with self.assertRaisesRegex(ValueError, "beyond pinned size"):
            module.verify_source(self.root, self.source)

    def test_wrong_uncompressed_digest_rejected(self):
        self.source["uncompressedSha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "uncompressed source mismatch"):
            module.verify_source(self.root, self.source)

    def test_parent_path_rejected(self):
        self.source["file"] = "../source.txt.gz"
        with self.assertRaisesRegex(ValueError, "basename"):
            module.verify_source(self.root, self.source)

    def test_windows_parent_path_rejected(self):
        self.source["file"] = "..\\source.txt.gz"
        with self.assertRaisesRegex(ValueError, "basename"):
            module.verify_source(self.root, self.source)

    def test_missing_source_rejected(self):
        (self.root / self.source["file"]).unlink()
        with self.assertRaisesRegex(ValueError, "Missing"):
            module.verify_source(self.root, self.source)

    def test_valid_output_accepted(self):
        module.validate_output(self.root, self.manifest)

    def test_corrupt_output_rejected(self):
        (self.root / self.manifest["file"]).write_bytes(self.binary[:-1])
        with self.assertRaisesRegex(ValueError, "size/hash"):
            module.validate_output(self.root, self.manifest)

    def test_wrong_magic_rejected_even_with_matching_hash(self):
        data = b"RTENGL01test-data"
        (self.root / self.manifest["file"]).write_bytes(data)
        self.manifest["sha256"] = hashlib.sha256(data).hexdigest()
        with self.assertRaisesRegex(ValueError, "magic"):
            module.validate_output(self.root, self.manifest)

    def test_wrong_manifest_identity_rejected(self):
        for key, value in (("schema", 1), ("format", "RTENLX01"), ("file", "english-other.bin")):
            with self.subTest(key=key):
                bad = {**self.manifest, key: value}
                with self.assertRaises(ValueError):
                    module.validate_output(self.root, bad)

    def test_missing_pinyin_rejected(self):
        self.manifest["counts"]["pinyin"] = 0
        with self.assertRaisesRegex(ValueError, "words and pinyin"):
            module.validate_output(self.root, self.manifest)

    def test_changed_manifest_rejected(self):
        different = copy.deepcopy(self.manifest)
        different["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "reviewed expected"):
            module.require_same_manifest(self.manifest, different)

    def test_no_change_accepted(self):
        module.require_same_manifest(self.manifest, copy.deepcopy(self.manifest))


if __name__ == "__main__":
    unittest.main()
