#!/usr/bin/env python3
"""EARANK01 overlay guards against the frozen frequency and ECDICT sources."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('build_english_ranking', Path(__file__).with_name('build-english-ranking.py'))
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


class RankingOverlay(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = M.compile_rows()

    def test_committed_runtime_matches_deterministic_compilation(self):
        self.assertEqual(M.OUTPUT.read_bytes(), M.image(self.rows))

    def test_contractions_replace_flat_placeholders_and_keep_pronoun_case(self):
        counts = M.frequencies()
        for word in ["don't", "i'm", "can't", "you're", "isn't"]:
            self.assertGreater(self.rows[word][0], counts["hello"], word)
        self.assertEqual(self.rows["i'm"][1], "I'm")
        self.assertEqual(self.rows["don't"][1], "")
        self.assertEqual(self.rows["you'v"], (1, "", "typo"))

    def test_archaic_words_are_demoted_but_formal_current_words_are_not(self):
        counts = M.frequencies()
        for word in ["thy", "thou", "hath", "couldst", "didst", "shalt", "unto"]:
            self.assertEqual(self.rows[word][2], "archaic", word)
            self.assertLess(self.rows[word][0], counts[word] // 100)
        for word in ["hitherto", "thereof", "whereupon", "art", "hyping", "anon", "triceratops"]:
            self.assertNotEqual(self.rows.get(word, (0, "", ""))[2], "archaic", word)

    def test_names_are_capitalized_without_capturing_inflections(self):
        self.assertEqual(self.rows["andrew"][1:], ("Andrew", "name"))
        self.assertEqual(self.rows["atlanta"][1:], ("Atlanta", "name"))
        self.assertEqual(self.rows["oklahoma"][1:], ("Oklahoma", "proper"))
        counts = M.frequencies()
        for word, display in [("monday", "Monday"), ("english", "English")]:
            self.assertEqual(self.rows[word], (counts[word], display, "proper"))
        for word in ["beers", "abler", "breasted", "aldermen", "foo", "will", "bill", "china"]:
            self.assertNotEqual(self.rows.get(word, (0, "", ""))[2], "name", word)

    def test_modern_words_carry_canonical_casing(self):
        self.assertEqual(self.rows["iphone"][1], "iPhone")
        self.assertEqual(self.rows["youtube"][1], "YouTube")
        self.assertIn("ok", self.rows)


if __name__ == '__main__':
    unittest.main()
