#!/usr/bin/env python3
"""Compile attributed ECDICT/CC-CEDICT source data without executing upstream code.

RTENLX02: 24-byte header (magic, word/pinyin/mapping counts, arena bytes),
36-byte word records (word/POS/definition/IPA offset-length pairs + rank),
16-byte pinyin records (key offset-length + mapping offset-count), 40-byte relations
(word ID, Chinese/reading/gloss string references, Chinese frequency, flags, ordinal),
then a UTF-8 string arena. All integers are little-endian and offsets are relative.
"""
import argparse
import csv
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import unicodedata
from collections import defaultdict

MAGIC = b"RTENLX02"
WORD = re.compile(r"[a-z](?:[a-z']{0,62}[a-z])?\Z")
# Everyday things are often two English words (cell phone, credit card). A
# phrase is admitted only as a complete CC-CEDICT gloss that is also an ECDICT
# headword; it is reachable from pinyin, never offered as a completion.
PHRASE = re.compile(r"(?=.{3,32}\Z)[a-z]+(?: [a-z]+){1,2}\Z")
ENTRY = re.compile(r"^(\S+) (\S+) \[([^]]+)\] /(.*)/$")
HANZI = re.compile(r"[\u3400-\u9fff\uf900-\ufaff]{1,16}\Z")
POS = re.compile(r"^(n|v|vt|vi|adj|adv|a|ad|prep|pron|conj|num|art|int|interj|aux|abbr|pl)\.\s*", re.I)


def source_bytes(path):
    raw = Path(path).read_bytes()
    return gzip.decompress(raw) if str(path).endswith(".gz") else raw


def digest(data):
    return hashlib.sha256(data).hexdigest()


def clean(text):
    return " ".join(text.replace("\\n", "\n").replace("\\r", "\n").split())


def annotation(row):
    # Keep POS independently so product preferences can hide it without parsing UI text.
    value = row.get("translation", "").replace("\\n", "\n").replace("\\r", "\n")
    lines = [clean(line) for line in value.splitlines() if clean(line)]
    parts = []
    meanings = []
    for line in lines[:2]:
        match = POS.match(line)
        if match:
            part = match.group(1).lower()
            part = {"a": "adj", "ad": "adv", "interj": "int"}.get(part, part)
            if part not in parts:
                parts.append(part)
            line = line[match.end():]
        meanings.append(line)
    if not parts:
        ratios = []
        for item in row.get("pos", "").split("/"):
            try:
                tag, amount = item.split(":", 1)
                ratios.append((float(amount), tag))
            except ValueError:
                continue
        if ratios:
            part = {"j": "adj", "r": "adv"}.get(max(ratios)[1], max(ratios)[1])
            if part in {"n", "v", "adj", "adv", "pron", "prep", "conj", "int", "num", "art"}:
                parts.append(part)
    meaning = clean("；".join(meanings)).replace(", ", "；").replace("，", "；")
    meaning = meaning[:96]
    phonetic = clean(row.get("phonetic", ""))[:64]
    return ("/".join(part + "." for part in parts), meaning, phonetic)


def pinyin_key(reading):
    result = []
    for char in unicodedata.normalize("NFD", reading.lower().replace("u:", "ü")):
        if char == "\u0308" and result and result[-1] == "u":
            result[-1] = "v"
        elif unicodedata.combining(char) or char in "12345 '\t":
            continue
        else:
            result.append(char)
    return "".join(result)


def numbered_reading(reading):
    syllables = []
    marks = {"\u0304": "1", "\u0301": "2", "\u030c": "3", "\u0300": "4"}
    for syllable in reading.lower().replace("u:", "ü").split():
        tone = next((c for c in syllable if c in "12345"), "5")
        tone = next((marks[c] for c in unicodedata.normalize("NFD", syllable) if c in marks), tone)
        syllables.append(pinyin_key(syllable) + tone)
    return " ".join(syllables)


def cedict_terms(data):
    entries = []
    for line in data.decode("utf-8-sig").splitlines():
        match = ENTRY.match(line)
        if not match:
            continue
        traditional, simplified, reading, gloss = match.groups()
        key = pinyin_key(reading)
        if not re.fullmatch(r"[a-z]{2,64}", key) or not HANZI.fullmatch(simplified):
            continue
        entries.append((traditional, simplified, reading, key, gloss.split("/")))
    return entries


def direct_words(gloss):
    for piece in gloss.split(";"):
        term = re.sub(r"\([^)]*\)", "", piece).strip().lower().strip(" .!?,")
        term = re.sub(r"^(?:to|a|an) ", "", term)
        # A whole copular single-word gloss has a clear English head (be like,
        # be fragrant, feel sad). Never pick a token out of a longer phrase.
        term = re.sub(r"^(?:be|feel) (?=[a-z]+$)", "", term)
        if WORD.fullmatch(term) or PHRASE.fullmatch(term):
            yield term


def pinyin_words(data):
    entries = defaultdict(set)
    for _, _, _, key, glosses in cedict_terms(data):
        for gloss in glosses:
            entries[key].update(direct_words(gloss))
    return {key: words for key, words in entries.items() if words}


def definition_terms(value):
    """Only complete Chinese gloss items; never substring or guessed segmentation."""
    value = value.replace("\\n", "\n").replace("\\r", "\n")
    for line in value.splitlines():
        line = POS.sub("", line.strip())
        for piece in re.split(r"[;；,，、]", line):
            piece = re.sub(r"\[[^]]*\]|\([^)]*\)|（[^）]*）", "", piece).strip()
            if HANZI.fullmatch(piece):
                yield piece


def chinese_frequencies(sources, wanted):
    """Counts by (word, numbered reading) and, for words whose two sources
    disagree only on a neutral tone (晚上 shang5/shang4), by (word, toneless key)."""
    result = {}
    for source in sources:
        for line in source.decode("utf-8-sig").splitlines():
            fields = line.split("\t")
            if len(fields) < 3 or fields[0] not in wanted:
                continue
            try:
                count = max(0, min(int(fields[2]), 0xffffffff))
            except ValueError:
                continue
            for pair in ((fields[0], numbered_reading(fields[1])), (fields[0], pinyin_key(fields[1]))):
                result[pair] = max(result.get(pair, 0), count)
    return result


def usage_flags(reading, gloss):
    dated = bool(re.search(r"\b(?:archaic|obsolete|literary|classical)\b|\(old\)", gloss, re.I))
    proper = reading[:1].isupper() or bool(re.search(r"\bsurname\b|\bvariant of\b", gloss, re.I))
    return (4 if dated else 0) | (8 if proper else 0)


def order_relations(values, records):
    """Chinese use comes first; diversify common headwords before rare senses."""
    groups = defaultdict(list)
    for value in values:
        word, hanzi, reading, gloss, frequency, flags, ordinal = value
        groups[(hanzi, reading)].append(value)
    for values in groups.values():
        # One common word answers better than a phrase (sorry before "excuse me").
        values.sort(key=lambda r: (bool(r[5] & 12), " " in r[0], bool(re.match(r"^\((?:of|in|for|when|used)\b", r[3], re.I)), -(r[5] & 3 == 3), -bool(r[5] & 1), records[r[0]][3] * (r[6] + 1), r[6], r[0], r[3]))
    groups = sorted(groups.values(), key=lambda group: (all(r[5] & 12 for r in group), -max(r[4] for r in group), group[0][1], group[0][2]))
    maximum = max((r[4] for group in groups for r in group), default=0)
    threshold = max(1, maximum // 128)
    common, uncommon = [], []
    for group in groups:
        modern = [r for r in group if not r[5] & 12]
        other = [r for r in group if r[5] & 12]
        if modern and max(r[4] for r in modern) >= threshold:
            common.append(modern)
            if other:
                uncommon.append(other)
        else:
            uncommon.append(group)
    ordered = []
    for bucket in (common, uncommon):
        for depth in range(max(map(len, bucket), default=0)):
            for group in bucket:
                if depth < len(group):
                    ordered.append(group[depth])
    return ordered


def compile_data(ecdict, cedict, frequency, chinese_zi=b"", chinese_words=b""):
    frequencies = {}
    for line in frequency.decode("utf-8-sig").splitlines():
        try:
            word, count = line.rsplit(" ", 1)
            if WORD.fullmatch(word) and int(count) > 0:
                frequencies[word] = int(count)
        except ValueError:
            continue
    ranks = {word: rank for rank, word in enumerate(sorted(frequencies, key=lambda word: (-frequencies[word], word)), 1)}
    terms = cedict_terms(cedict)
    pinyin = pinyin_words(cedict)
    related = set().union(*pinyin.values()) if pinyin else set()
    records = {}
    qualities = {}
    lemmas = {}
    definitions = {}
    for row in csv.DictReader(io.StringIO(ecdict.decode("utf-8-sig"))):
        original = row.get("word", "")
        word = original.lower()
        if not (WORD.fullmatch(word) or PHRASE.fullmatch(word)) or (word not in ranks and word not in related):
            continue
        part, meaning, ipa = annotation(row)
        if not meaning:
            continue
        raw_ranks = []
        for field in ("frq", "bnc"):
            try:
                value = int(row.get(field, ""))
                if value > 0:
                    raw_ranks.append(value)
            except ValueError:
                pass
        rank = ranks.get(word, 1_000_000 + min(raw_ranks, default=1_000_000))
        # Exact lowercase dictionary entries outrank case-folded acronyms. A
        # stable original-spelling tie break makes case variants order independent.
        quality = (original != word, original)
        if word not in qualities or quality < qualities[word]:
            qualities[word] = quality
            records[word] = (part, meaning, ipa, rank)
            definitions[word] = set(definition_terms(row.get("translation", "")))
            forms = dict(item.split(":", 1) for item in row.get("exchange", "").split("/") if ":" in item)
            lemmas[word] = forms.get("0", "").lower()
    # Some inflected forms in the frozen source have only an abbreviation gloss
    # (helps is one). Prefer its explicitly supplied lemma, never a guessed stem.
    # Retain the inflected form's own IPA: a lemma's pronunciation may be wrong.
    for word, lemma in lemmas.items():
        if records[word][0] == "abbr." and lemma in records and records[lemma][0] != "abbr.":
            part, meaning, _, _ = records[lemma]
            _, _, ipa, rank = records[word]
            records[word] = (part, meaning, ipa, rank)
    words = sorted(records)
    ids = {word: index for index, word in enumerate(words)}
    wanted = {spelling for term in terms for spelling in term[:2]}
    chinese = chinese_frequencies((chinese_zi, chinese_words), wanted)
    by_hanzi = defaultdict(list)
    relations = defaultdict(dict)
    direct_readings = defaultdict(set)

    def add(word, term, gloss, flags, ordinal):
        if word not in ids:
            return
        traditional, simplified, reading, key, _ = term
        count = max(chinese.get((spelling, numbered_reading(reading)), 0) for spelling in (traditional, simplified))
        if not count:
            count = max(chinese.get((spelling, key), 0) for spelling in (traditional, simplified))
        identity = (word, simplified, reading)
        value = (word, simplified, reading, clean(gloss)[:512], count, flags, ordinal)
        previous = relations[key].get(identity)
        if previous:
            # Corroboration may combine sources, but usage/ordinal must describe
            # the retained sense. An old synonym must not mark a modern one old.
            preferred = min((previous, value), key=lambda r: (bool(r[5] & 12), not bool(r[5] & 1), r[6], r[3]))
            combined_flags = ((previous[5] | flags) & 3) | (preferred[5] & 12)
            value = (*preferred[:5], combined_flags, preferred[6])
        relations[key][identity] = value

    for term in terms:
        traditional, simplified, reading, key, glosses = term
        for spelling in {traditional, simplified}:
            by_hanzi[spelling].append(term)
        for ordinal, gloss in enumerate(glosses):
            for word in direct_words(gloss):
                add(word, term, gloss, 1 | usage_flags(reading, gloss), ordinal)
                direct_readings[(word, simplified)].add(numbered_reading(reading))
    for word, meanings in definitions.items():
        if lemmas.get(word) and lemmas[word] != word:
            continue
        for meaning in sorted(meanings):
            choices = by_hanzi.get(meaning, ())
            readings = {numbered_reading(term[2]) for term in choices}
            for term in choices:
                supported = direct_readings.get((word, term[1]))
                if (supported and numbered_reading(term[2]) not in supported) or (not supported and len(readings) > 1):
                    continue
                # This is an explicit ECDICT Chinese definition, not a claim that
                # a token was extracted from an unrelated CC-CEDICT phrase.
                add(word, term, meaning, 2 | usage_flags(term[2], ""), 0)
    pinyin = {key: order_relations(values.values(), records) for key, values in sorted(relations.items()) if values}
    arena = bytearray()
    word_table = bytearray()
    key_table = bytearray()
    mappings = bytearray()
    strings = {}

    def string(table, value):
        encoded = value.encode("utf-8")
        if value not in strings:
            strings[value] = (len(arena), len(encoded))
            arena.extend(encoded)
        table.extend(struct.pack("<II", *strings[value]))

    for word in words:
        part, meaning, ipa, rank = records[word]
        for value in (word, part, meaning, ipa):
            string(word_table, value)
        word_table.extend(struct.pack("<I", rank))
    for key, values in pinyin.items():
        string(key_table, key)
        key_table.extend(struct.pack("<II", len(mappings) // 40, len(values)))
        for word, hanzi, reading, gloss, count, flags, ordinal in values:
            mappings.extend(struct.pack("<I", ids[word]))
            for text in (hanzi, reading, gloss):
                string(mappings, text)
            mappings.extend(struct.pack("<III", count, flags, ordinal))
    header = MAGIC + struct.pack("<IIII", len(words), len(pinyin), len(mappings) // 40, len(arena))
    output = header + word_table + key_table + mappings + arena
    if len(output) > 64 * 1024 * 1024:
        raise ValueError("The compiled lexicon exceeds the runtime's 64 MiB bound")
    return output, {"words": len(words), "pinyin": len(pinyin), "mappings": len(mappings) // 40}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ecdict", required=True, type=Path)
    parser.add_argument("--cedict", required=True, type=Path)
    parser.add_argument("--frequency", type=Path, default=Path(__file__).resolve().parents[2] / "data/dict/raw/english/frequency.txt")
    parser.add_argument("--chinese-zi", type=Path)
    parser.add_argument("--chinese-words", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    args = parser.parse_args()
    inputs = {name: source_bytes(getattr(args, name)) for name in ("ecdict", "cedict", "frequency")}
    for name in ("chinese_zi", "chinese_words"):
        if getattr(args, name):
            inputs[name] = source_bytes(getattr(args, name))
    output, counts = compile_data(**inputs)
    sources = {}
    for name, content in inputs.items():
        path = getattr(args, name)
        sources[name] = {"file": path.name, "sha256": digest(path.read_bytes()), "content_sha256": digest(content)}
    manifest = {"schema": 2, "format": MAGIC.decode("ascii"), "file": args.output.name,
                "bytes": len(output), "sha256": digest(output), "counts": counts, "sources": sources}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(output)
    args.manifest.write_text(json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(manifest, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
