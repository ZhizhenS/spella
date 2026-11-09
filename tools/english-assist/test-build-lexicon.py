#!/usr/bin/env python3
"""Deterministic compiler tests using small synthetic source data only."""
import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location("build_lexicon", Path(__file__).with_name("build-lexicon.py"))
compiler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compiler)


class LexiconCompilerTests(unittest.TestCase):
    @staticmethod
    def entries(output):
        words, keys, mappings, _ = struct.unpack_from("<IIII", output, 8)
        arena = 24 + words * 36 + keys * 16 + mappings * 4
        result = {}
        for index in range(words):
            values = []
            for offset in (0, 8, 16, 24):
                start, size = struct.unpack_from("<II", output, 24 + index * 36 + offset)
                values.append(output[arena + start:arena + start + size].decode())
            result[values[0]] = values[1:]
        return result

    def test_lowercase_word_wins_over_acronym_independent_of_source_order(self):
        header = "word,translation,phonetic,pos,bnc,frq,exchange\n"
        rows = ["HELPS,abbr. 打印解决方案,,,,,\n", "helps,v. 帮助,helps,,,,\n"]
        frequency = b"helps 100\n"
        first = compiler.compile_data((header + ''.join(rows)).encode(), b"", frequency)[0]
        second = compiler.compile_data((header + ''.join(reversed(rows))).encode(), b"", frequency)[0]
        self.assertEqual(first, second)
        self.assertEqual(self.entries(first)["helps"][:2], ["v.", "帮助"])

    def test_explicit_source_lemma_repairs_abbreviation_gloss_without_copying_ipa(self):
        rows = "word,translation,phonetic,pos,bnc,frq,exchange\nhelp,v. 帮助,help,,,,\nhelps,abbr. 打印方案,helps,,,,0:help/1:3s\n"
        output = compiler.compile_data(rows.encode(), b"", b"help 100\nhelps 50\n")[0]
        self.assertEqual(self.entries(output)["helps"], ["v.", "帮助", "helps"])

    def test_annotation_fields_are_separate_and_bounded(self):
        self.assertEqual(compiler.annotation({"translation": r"n. 苹果, 苹果树\nv. 示例", "phonetic": "ˈæpəl"}),
                         ("n./v.", "苹果；苹果树；示例", "ˈæpəl"))
        self.assertEqual(compiler.annotation({"translation": "形容词", "pos": "n:20/j:80"})[0], "adj.")
        self.assertLessEqual(len(compiler.annotation({"translation": "中" * 500})[1]), 96)

    def test_pinyin_uses_plain_source_and_drops_non_word_glosses(self):
        source = "你好 你好 [ni3 hao3] /Hello!/Hi!/(polite) hello/How are you?/long phrase/\n苹果 苹果 [ping2 guo3] /apple/\n".encode()
        entries = compiler.pinyin_words(source)
        self.assertEqual(entries["nihao"], {"hello", "hi"})
        self.assertEqual(entries["pingguo"], {"apple"})
        self.assertNotIn("nh", entries)

    def test_binary_generation_is_deterministic_and_keeps_pinyin_words(self):
        csv = "word,translation,phonetic,pos,bnc,frq\napple,n. 苹果,æpəl,n:100,20,20\nhello,int. 你好,həlo,,30,30\nhelp,v. 帮助,help,v:100,10,10\nunused,n. 不使用,,,50,50\n".encode()
        cedict = "你好 你好 [ni3 hao3] /hello/\n苹果 苹果 [ping2 guo3] /apple/\n".encode()
        frequency = b"help 100\napple 50\n"
        output, counts = compiler.compile_data(csv, cedict, frequency)
        self.assertEqual(output, compiler.compile_data(csv, cedict, frequency)[0])
        self.assertEqual(counts, {"words": 3, "pinyin": 2, "mappings": 2})
        self.assertEqual(output[:8], b"RTENLX01")
        words, pinyin, mappings, arena = struct.unpack_from("<IIII", output, 8)
        self.assertEqual(len(output), 24 + words * 36 + pinyin * 16 + mappings * 4 + arena)


if __name__ == "__main__":
    unittest.main()
