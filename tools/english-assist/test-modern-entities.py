#!/usr/bin/env python3
"""Offline catalog guard tests, using only isolated temporary source fixtures."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("modern-entities.py")
SPEC = importlib.util.spec_from_file_location("modern_entities", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ReviewedModernEntities(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="english-assist-entities-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        original_catalog, original_snapshot = MODULE.CATALOG, MODULE.SNAPSHOT
        self.data = json.loads(original_catalog.read_text(encoding="utf-8"))
        self.catalog, self.snapshot, self.runtime = (root / name for name in ("catalog.json", "snapshot.json.gz", "runtime.tsv"))
        self.snapshot.write_bytes(original_snapshot.read_bytes())
        for name, value in (("CATALOG", self.catalog), ("SNAPSHOT", self.snapshot), ("RUNTIME", self.runtime)):
            original = getattr(MODULE, name)
            setattr(MODULE, name, value)
            self.addCleanup(setattr, MODULE, name, original)
        network = patch.object(MODULE.urllib.request, "urlopen", side_effect=AssertionError("offline check attempted network"))
        network.start()
        self.addCleanup(network.stop)
        self.save()

    def save(self):
        self.catalog.write_text(json.dumps(self.data, ensure_ascii=False), encoding="utf-8")
        self.runtime.write_text(MODULE.image(self.data["entries"]), encoding="utf-8", newline="\n")

    def test_approved_snapshot_has_verified_apple_and_anthropic(self):
        result = MODULE.checked()
        self.assertEqual(len(result["entries"]), 51)
        entries = {entry["text"]: entry for entry in result["entries"]}
        self.assertEqual(entries["Apple"]["wikidata_id"], "Q312")
        self.assertEqual(entries["Anthropic"]["definition_zh"], "美国人工智能公司；Claude 开发商")

    def test_snapshot_digest_must_match(self):
        self.data["snapshot"]["sha256"] = "0" * 64
        self.save()
        with self.assertRaises(ValueError):
            MODULE.checked()

    def test_revision_must_exist_in_frozen_source(self):
        self.data["entries"][0]["wikidata_revision"] += 1
        self.save()
        with self.assertRaises(ValueError):
            MODULE.checked()

    def test_well_formed_but_unverified_id_is_rejected(self):
        self.data["entries"][0]["wikidata_id"] = "Q999999999999999"
        self.save()
        with self.assertRaises(ValueError):
            MODULE.checked()

    def test_duplicate_alias_cannot_hide_another_entity(self):
        self.data["entries"][1]["aliases"] = self.data["entries"][0]["aliases"][:]
        self.save()
        with self.assertRaises(ValueError):
            MODULE.checked()

    def test_embedded_runtime_must_match_reviewed_catalog(self):
        self.runtime.write_text("# stale runtime\n", encoding="utf-8")
        with self.assertRaises(ValueError):
            MODULE.checked()


if __name__ == "__main__":
    unittest.main()
