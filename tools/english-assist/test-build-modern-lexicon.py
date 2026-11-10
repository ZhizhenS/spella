#!/usr/bin/env python3
"""Isolated lexical-source/compiler regressions; no network or installed state."""
import copy
import gzip
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("modern_compiler", Path(__file__).with_name("build-modern-lexicon.py"))
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def editorial(entries=()):
    return {"schema": 1, "definition_origin": "project-editorial", "entries": list(entries)}


def record(word="IDK", gloss="Initialism of I don't know.", pos="phrase", edition="en", definition="我不知道", tags=None, full="I don't know"):
    sense = {"glosses": [gloss], "tags": ["Internet", "abbreviation", "initialism"] if tags is None else tags}
    if full:
        sense["alt_of"] = [{"word": full}]
    sense["sense_id"] = edition + "-" + M.digest(M.canonical([word, gloss, pos, edition]))[:40]
    if edition == "en" and definition:
        sense["translations_zh"] = [{"code": "cmn", "word": definition}]
    elif edition == "zh":
        sense["glosses"] = [definition or gloss]
    return {"edition": edition, "word": word, "pos": pos, "source_line": 7, "source_entry_sha256": "a" * 64, "senses": [sense]}


def override(rec, definition="我不知道", priority=2000, **extra):
    sense = rec["senses"][0]
    return {"word": rec["word"], "pos": rec["pos"], "sense_id": sense["sense_id"], "glosses": sense["glosses"], "full_form": M.full_form(sense), "definition_zh": definition, "priority": priority, **extra}


def rows(image):
    return [line.split("\t") for line in image.decode().splitlines()[1:]]


class ModernCompilerTests(unittest.TestCase):
    def test_projection_excludes_other_languages_and_nonlexical_fields(self):
        entry = {"word": "rizz", "lang_code": "en", "pos": "noun", "sounds": [{"audio": "x"}], "senses": [{"glosses": ["Charm."], "tags": ["slang"], "examples": [{"text": "not part of the lexical subset"}]}]}
        projected = M.project(entry, 19)
        self.assertEqual(projected["source_line"], 19)
        self.assertNotIn("sounds", projected)
        self.assertNotIn("examples", projected["senses"][0])
        self.assertIsNone(M.project(entry | {"lang_code": "fr"}, 19))

    def test_entry_translation_requires_unique_exact_sense_gloss(self):
        entry = {"word": "cool", "lang_code": "en", "pos": "adj", "senses": [{"glosses": ["Good.", "Pleasant."], "tags": ["slang"]}, {"glosses": ["Good.", "Fashionable."], "tags": ["slang"]}], "translations": [{"code": "cmn", "word": "好", "sense": "Good"}, {"code": "cmn", "word": "时髦的", "sense": "Fashionable"}]}
        senses = M.project(entry, 1)["senses"]
        self.assertEqual(senses[0]["translations_zh"], [])
        self.assertEqual([t["word"] for t in senses[1]["translations_zh"]], ["时髦的"])

    def test_relation_only_chinese_is_not_a_semantic_translation(self):
        for gloss in ("IMO的另一種寫法。", "ASAP的大小寫替代形式。", "doomscroll的現在時分詞和動名詞"):
            self.assertEqual(M.chinese_gloss({"glosses": [gloss], "tags": ["alt-of"]}), "")
        self.assertEqual(M.chinese_gloss({"glosses": ['I don\'t know (“我不知道”) 的首字母縮略詞'], "tags": ["form-of"]}), "我不知道")
        self.assertEqual(M.chinese_gloss({"glosses": ["尽快（as soon as possible 的缩略）。"], "tags": ["form-of"]}), "尽快")

    def test_embedded_example_is_not_in_runtime_definition(self):
        sense = {"glosses": ["开始讲话\n例：Shoot!"], "tags": ["slang"]}
        self.assertEqual(M.chinese_gloss(sense), "开始讲话")

    def test_project_editorial_is_bound_to_exact_source_sense(self):
        rec = record(definition="")
        item = override(rec)
        image, _, audit = M.compile_records([rec], editorial([item]))
        self.assertEqual(rows(image)[0][3], "我不知道")
        self.assertEqual(audit[0]["definition_origin"], "project-editorial")
        for field, replacement in (("word", "IMO"), ("pos", "noun"), ("glosses", ["A different meaning."]), ("full_form", "other")):
            changed = copy.deepcopy(item)
            changed[field] = replacement
            with self.assertRaises(ValueError):
                M.compile_records([rec], editorial([changed]))

    def test_override_does_not_leak_into_homograph_senses(self):
        everyday = record(word="IMO", gloss="Initialism of in my opinion.", full="in my opinion", pos="prep_phrase", definition="")
        maritime = record(word="IMO", gloss="Initialism of International Maritime Organization.", full="International Maritime Organization", pos="name", definition="国际海事组织")
        image, _, audit = M.compile_records([everyday, maritime], editorial([override(everyday, "依我看")]))
        self.assertEqual({r[3] for r in rows(image)}, {"依我看", "国际海事组织"})
        self.assertEqual(len(audit), 2)

    def test_equivalent_expansion_merges_but_other_expansion_survives(self):
        en = record()
        zh = record(edition="zh", definition='I don\'t know (“我不知道”) 的首字母縮略詞')
        other = record(word="IDK", gloss="A different expansion.", full="I do know", definition="我知道")
        image, counts, audit = M.compile_records([en, zh, other], editorial([override(en)]))
        self.assertEqual(len(rows(image)), 2)
        native = next(a for a in audit if a["full_form"] == "I don't know")
        self.assertEqual(native["edition"], "zh")
        self.assertEqual(native["priority"], 2000)
        self.assertEqual(len(native["equivalent_sources"]), 1)
        self.assertEqual(counts["cross_source_merged_senses"], 1)

    def test_later_old_sense_cannot_inherit_modern_priority(self):
        modern = record(word="rizz", gloss="Charm.", pos="noun", full="", definition="魅力", tags=["slang"])
        old = record(word="rizz", gloss="Rolling paper.", pos="noun", full="", definition="卷烟纸", tags=["slang", "dated", "rare"])
        image, _, _ = M.compile_records([old, modern], editorial())
        out = rows(image)
        self.assertEqual(out[0][3], "魅力")
        self.assertGreater(int(out[0][-1]), int(out[1][-1]))

    def test_missing_chinese_unknown_pos_and_template_artifacts_are_not_admitted(self):
        image, counts, _ = M.compile_records([record(definition=""), record(word="NEW", pos="unmapped"), record(word="DEF", definition="Template:之縮寫"), record(word="DUN", definition="do 的過去分詞")], editorial())
        self.assertEqual(rows(image), [])
        self.assertEqual(counts["missing_chinese_senses"], 1)
        self.assertEqual(counts["unsupported_pos_senses"], 1)
        self.assertEqual(counts["invalid_lexical_chinese_senses"], 2)

    def test_editorial_bad_definition_priority_or_identity_fails(self):
        rec = record()
        for changes in ({"definition_zh": "网络用语"}, {"definition_zh": "not Chinese"}, {"definition_zh": "含\t制表符"}, {"priority": -1}, {"priority": True}, {"priority": 2**32}):
            with self.assertRaises(ValueError):
                M.compile_records([rec], editorial([override(rec, **changes)]))
        invalid = copy.deepcopy(rec)
        invalid["senses"][0]["sense_id"] = "bad"
        with self.assertRaises(ValueError):
            M.compile_records([invalid], editorial())

    def test_runtime_case_punctuation_unicode_expansion_and_total_order(self):
        first = record(word="TL;DR", full="too long; didn’t read", definition="长话短说")
        second = record(word="IMO", full="in my opinion", definition="依我看")
        image, _, _ = M.compile_records([first, second], editorial())
        out = rows(image)
        self.assertEqual([r[0] for r in out], ["imo", "tldr"])
        self.assertEqual(out[-1][1], "TL;DR")
        self.assertEqual(out[-1][5], "too long; didn’t read")
        self.assertTrue(all(len(row) == 9 for row in out))

    def test_register_policy_excludes_offensive_and_archaic_and_limits_vulgar(self):
        cases = [
            ("noob", {"tags": ["Internet", "derogatory"]}, "菜鸟", ""),
            ("karen", {"tags": ["offensive", "slang"]}, "普通释义", "offensive"),
            ("hout", {"tags": ["slang"], "raw_tags": ["種族歧視"]}, "普通释义", "offensive"),
            ("tran", {"tags": ["informal"]}, "跨性别人士", "offensive"),
            ("thee", {"tags": ["archaic"]}, "你", "obsolete"),
            ("twat", {"tags": ["slang", "vulgar"]}, "笨蛋", "vulgar"),
            ("naai", {"tags": ["slang"]}, "(南非) 性交", "vulgar"),
            ("type shit", {"tags": ["slang"]}, "确实", "vulgar"),
            ("vibe", {"tags": ["slang"]}, "氛围", ""),
            ("os", {"tags": ["form-of"]}, "操作系統", ""),
        ]
        for word, sense, definition, expected in cases:
            with self.subTest(word=word):
                self.assertEqual(M.register(word, sense, definition), expected)
        self.assertEqual(M.usage({"tags": ["slang"], "raw_tags": ["過時俚語"]}), "旧义")

    def test_reviewed_unlabelled_seed_does_not_invent_upstream_usage(self):
        source = {"word": "sus", "lang_code": "en", "pos": "adj", "senses": [{"glosses": ["Suspicious."]}]}
        projected = M.project(source, 3)
        self.assertNotIn("tags", projected["senses"][0])
        self.assertEqual(M.usage(projected["senses"][0]), "词汇")

    def test_literal_gloss_expansion_requires_its_own_source_evidence(self):
        rec = record(word="POV", full="", gloss="Initialism of point of view.")
        item = override(rec, "观点", display_full_form="point of view")
        image, _, _ = M.compile_records([rec], editorial([item]))
        self.assertEqual(rows(image)[0][5], "point of view")
        item["display_full_form"] = "made up expansion"
        with self.assertRaises(ValueError):
            M.compile_records([rec], editorial([item]))

    def test_priority_only_review_keeps_unchanged_native_chinese_origin(self):
        rec = record(edition="zh", definition="卷烟纸", full="", tags=["slang"])
        item = override(rec, "卷烟纸", priority=50, definition_from_upstream_chinese=True)
        _, counts, audit = M.compile_records([rec], editorial([item]))
        self.assertEqual(audit[0]["definition_origin"], "upstream-chinese-edition-gloss")
        self.assertEqual(counts["editorial_chinese_senses"], 0)
        self.assertEqual(audit[0]["priority"], 50)
        item["definition_zh"] = "改变了的中文"
        with self.assertRaises(ValueError):
            M.compile_records([rec], editorial([item]))

    def test_frozen_subset_digest_rejects_actual_tempfile_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            raw = Path(directory)
            content = M.canonical(record()) + b"\n"
            blob = gzip.compress(content, mtime=0)
            subset = raw / "english-modern-senses.jsonl.gz"
            subset.write_bytes(blob)
            meta = {"schema": 1, "data_license": "CC-BY-SA-4.0", "subset": {"file": subset.name, "bytes": len(blob), "sha256": M.digest(blob), "content_bytes": len(content), "content_sha256": M.digest(content)}, "extraction": {"selected_entries": 1, "selected_senses": 1}}
            M.write_json(raw / "SOURCES.json", meta)
            self.assertEqual(len(M.source_records(raw)[1]), 1)
            changed = bytearray(blob)
            changed[-1] ^= 1
            subset.write_bytes(changed)
            with self.assertRaisesRegex(ValueError, "compressed digest"):
                M.source_records(raw)

    def test_explicit_full_extractor_checks_source_sha_and_preserves_line_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source = base / "source.gz"
            content = b'{"lang_code":"fr","word":"non"}\n' + M.canonical({"word": "idk", "lang_code": "en", "pos": "phrase", "senses": [{"glosses": ["I do not know."], "tags": ["Internet"]}]}) + b"\n"
            source.write_bytes(gzip.compress(content, mtime=0))
            meta = {"url": M.SOURCE_URL, "bytes": source.stat().st_size, "sha256": M.file_digest(source), "etag": '"fixture"', "last_modified": "fixture", "complete": True}
            M.write_json(base / "meta.json", meta)
            M.extract(source, base / "meta.json", base / "projection")
            frozen, records = M.source_records(base / "projection")
            self.assertEqual(frozen["extraction"]["source_lines"], 2)
            self.assertEqual(records[0]["source_line"], 2)
            self.assertEqual(records[0]["source_entry_sha256"], M.digest(content.splitlines(keepends=True)[1]))
            source.write_bytes(gzip.compress(content + b"\n", mtime=0))
            with self.assertRaisesRegex(ValueError, "digest/metadata"):
                M.extract(source, base / "meta.json", base / "other")


if __name__ == "__main__":
    unittest.main()
