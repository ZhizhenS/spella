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
        arena = 24 + words * 36 + keys * 16 + mappings * 40
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
        self.assertEqual(output[:8], b"RTENLX02")
        words, pinyin, mappings, arena = struct.unpack_from("<IIII", output, 8)
        self.assertEqual(len(output), 24 + words * 36 + pinyin * 16 + mappings * 40 + arena)

    @staticmethod
    def relations(output, key):
        words, keys, count, _ = struct.unpack_from("<IIII", output, 8)
        key_at = 24 + words * 36
        hit_at = key_at + keys * 16
        arena = hit_at + count * 40
        def text(at):
            offset, size = struct.unpack_from("<II", output, at)
            return output[arena + offset:arena + offset + size].decode()
        for index in range(keys):
            at = key_at + index * 16
            if text(at) != key:
                continue
            first, length = struct.unpack_from("<II", output, at + 8)
            result = []
            for index in range(first, first + length):
                at = hit_at + index * 40
                word_id = struct.unpack_from("<I", output, at)[0]
                result.append((text(24 + word_id * 36), text(at + 4), text(at + 12), text(at + 20), *struct.unpack_from("<III", output, at + 28)))
            return result
        return []

    def test_common_chinese_senses_beat_rare_characters_even_when_the_english_is_frequent(self):
        csv = "word,translation,phonetic,pos,bnc,frq,exchange\nthink,v. 想,,,,,\nlike,prep. 像,,,,,\nfragrant,adj. 香,,,,,\nelephant,n. 象,,,,,\nschool,n. 学校,,,,,\nhelp,v. 帮助,,,,,\n".encode()
        cedict = "想 想 [xiang3] /to think/\n像 像 [xiang4] /to resemble/to be like/\n香 香 [xiang1] /fragrant/\n象 象 [xiang4] /elephant/\n庠 庠 [xiang2] /school/\n襄 襄 [xiang1] /to help/\n".encode()
        frequency = b"school 999999\nhelp 999998\nthink 30\nlike 20\nelephant 10\nfragrant 1\n"
        chinese = "想\txiǎng\t342401\n像\txiàng\t26756\n香\txiāng\t14433\n象\txiàng\t5515\n襄\txiāng\t571\n庠\txiáng\t411\n".encode()
        output, _ = compiler.compile_data(csv, cedict, frequency, chinese)
        relations = self.relations(output, "xiang")
        positions = {row[0]: i for i, row in enumerate(relations)}
        for common in ("think", "like", "fragrant", "elephant"):
            self.assertLess(positions[common], positions["school"])
            self.assertLess(positions[common], positions["help"])
        like = next(row for row in relations if row[0] == "like")
        self.assertEqual(like[1:4], ("像", "xiang4", "to be like"))
        self.assertEqual(like[5] & 3, 3)

    def test_complete_chinese_gloss_items_are_required_for_reverse_links(self):
        self.assertEqual(set(compiler.definition_terms(r"v. 想；像\nn. 想法；梦想")), {"想", "像", "想法", "梦想"})
        self.assertNotIn("想", set(compiler.definition_terms("v. 联想；设想；有所想")))
        self.assertEqual(list(compiler.direct_words("to be like")), ["like"])
        self.assertEqual(list(compiler.direct_words("to be like a school")), [])

    def test_chinese_prior_keeps_polyphonic_readings_separate(self):
        csv = "word,translation,phonetic,pos,bnc,frq,exchange\nphase,n. 相,,,,,\nmutually,adv. 互相,,,,,\n".encode()
        cedict = "相 相 [xiang1] /mutually/\n相 相 [xiang4] /phase/\n".encode()
        chinese = "相\txiāng\t27861\n相\txiàng\t208\n".encode()
        output, _ = compiler.compile_data(csv, cedict, b"phase 1000\nmutually 2\n", chinese)
        rows = self.relations(output, "xiang")
        self.assertEqual([(r[0], r[2], r[4]) for r in rows], [("mutually", "xiang1", 27861), ("phase", "xiang4", 208)])
        self.assertEqual(compiler.numbered_reading("lǜ sè"), "lv4 se4")
        self.assertEqual(compiler.numbered_reading("lu:4 se4"), "lv4 se4")

    def test_reverse_glosses_do_not_add_every_inflected_form(self):
        csv = "word,translation,phonetic,pos,bnc,frq,exchange\nthink,v. 想,,,,,\nthought,v. 想,,,,,0:think\n".encode()
        output, _ = compiler.compile_data(csv, "想 想 [xiang3] /to think/\n".encode(), b"thought 100\nthink 10\n")
        self.assertEqual([row[0] for row in self.relations(output, "xiang")], ["think"])

    def test_low_english_frequency_relations_are_not_destroyed_by_a_key_level_64_cap(self):
        names = ["word" + chr(97 + i // 26) + chr(97 + i % 26) for i in range(80)]
        csv = ("word,translation,phonetic,pos,bnc,frq\n" + "".join(word + ",n. 香,,,,\n" for word in names)).encode()
        cedict = ("香 香 [xiang1] /" + "/".join(names) + "/\n").encode()
        frequency = "".join(f"{word} {i + 1}\n" for i, word in enumerate(names)).encode()
        output, counts = compiler.compile_data(csv, cedict, frequency)
        self.assertEqual(counts["mappings"], 80)
        self.assertEqual({row[0] for row in self.relations(output, "xiang")}, set(names))

    def test_context_limited_senses_do_not_outrank_unqualified_senses(self):
        csv = "word,translation,phonetic,pos,bnc,frq\nsound,adj. 香,,,,\nfragrant,adj. 香,,,,\n".encode()
        cedict = "香 香 [xiang1] /fragrant/(of sleep) sound/\n".encode()
        output, _ = compiler.compile_data(csv, cedict, b"sound 10000\nfragrant 1\n")
        self.assertEqual(self.relations(output, "xiang")[0][0], "fragrant")

    def test_retained_sense_keeps_its_own_usage_flags_and_ordinal(self):
        csv = "word,translation,phonetic,pos,bnc,frq\ngrandfather,n. 祖父,,,,\nflash,v. 闪光,,,,\n".encode()
        cedict = "阿公 阿公 [a1 gong1] /(old) grandfather/(Taiwanese) grandfather/\n掣電 掣电 [che4 dian4] /to flash/a flash (literary)/\n".encode()
        output, _ = compiler.compile_data(csv, cedict, b"grandfather 100\nflash 10\n")
        grandfather = self.relations(output, "agong")[0]
        self.assertEqual(grandfather[3], "(Taiwanese) grandfather")
        self.assertEqual(grandfather[5] & 12, 0)
        self.assertEqual(grandfather[6], 1)
        flash = self.relations(output, "chedian")[0]
        self.assertEqual(flash[3], "to flash")
        self.assertEqual(flash[5] & 12, 0)


if __name__ == "__main__":
    unittest.main()
