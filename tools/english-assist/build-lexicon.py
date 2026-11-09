#!/usr/bin/env python3
"""Compile attributed ECDICT/CC-CEDICT source data without executing upstream code.

RTENLX01: 24-byte header (magic, word/pinyin/mapping counts, arena bytes),
36-byte word records (word/POS/definition/IPA offset-length pairs + rank),
16-byte pinyin records (key offset-length + mapping offset-count), u32 word IDs,
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

MAGIC = b"RTENLX01"
WORD = re.compile(r"[a-z](?:[a-z']{0,62}[a-z])?\Z")
ENTRY = re.compile(r"^\S+ (\S+) \[([^]]+)\] /(.*)/$")
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


def pinyin_words(data):
    entries = {}
    for line in data.decode("utf-8-sig").splitlines():
        match = ENTRY.match(line)
        if not match:
            continue
        key = re.sub(r"[1-5\s]", "", match.group(2).lower().replace("u:", "v").replace("ü", "v"))
        if not re.fullmatch(r"[a-z]{2,64}", key):
            continue
        words = entries.setdefault(key, set())
        for gloss in match.group(3).split("/"):
            for term in gloss.split(";"):
                term = re.sub(r"\([^)]*\)", "", term).strip().lower().strip(" .!?,")
                term = re.sub(r"^(?:to|a|an) ", "", term)
                if WORD.fullmatch(term):
                    words.add(term)
    return {key: words for key, words in entries.items() if words}


def compile_data(ecdict, cedict, frequency):
    frequencies = {}
    for line in frequency.decode("utf-8-sig").splitlines():
        try:
            word, count = line.rsplit(" ", 1)
            if WORD.fullmatch(word) and int(count) > 0:
                frequencies[word] = int(count)
        except ValueError:
            continue
    ranks = {word: rank for rank, word in enumerate(sorted(frequencies, key=lambda word: (-frequencies[word], word)), 1)}
    pinyin = pinyin_words(cedict)
    related = set().union(*pinyin.values()) if pinyin else set()
    records = {}
    qualities = {}
    lemmas = {}
    for row in csv.DictReader(io.StringIO(ecdict.decode("utf-8-sig"))):
        original = row.get("word", "")
        word = original.lower()
        if not WORD.fullmatch(word) or (word not in ranks and word not in related):
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
    pinyin = {key: sorted((word for word in values if word in ids), key=lambda word: (records[word][3], word))[:64]
              for key, values in pinyin.items()}
    pinyin = {key: values for key, values in sorted(pinyin.items()) if values}
    arena = bytearray()
    word_table = bytearray()
    key_table = bytearray()
    mappings = bytearray()

    def string(table, value):
        encoded = value.encode("utf-8")
        table.extend(struct.pack("<II", len(arena), len(encoded)))
        arena.extend(encoded)

    for word in words:
        part, meaning, ipa, rank = records[word]
        for value in (word, part, meaning, ipa):
            string(word_table, value)
        word_table.extend(struct.pack("<I", rank))
    for key, values in pinyin.items():
        string(key_table, key)
        key_table.extend(struct.pack("<II", len(mappings) // 4, len(values)))
        for word in values:
            mappings.extend(struct.pack("<I", ids[word]))
    header = MAGIC + struct.pack("<IIII", len(words), len(pinyin), len(mappings) // 4, len(arena))
    output = header + word_table + key_table + mappings + arena
    if len(output) > 64 * 1024 * 1024:
        raise ValueError("The compiled lexicon exceeds the runtime's 64 MiB bound")
    return output, {"words": len(words), "pinyin": len(pinyin), "mappings": len(mappings) // 4}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ecdict", required=True, type=Path)
    parser.add_argument("--cedict", required=True, type=Path)
    parser.add_argument("--frequency", type=Path, default=Path(__file__).resolve().parents[2] / "data/dict/raw/english/frequency.txt")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    args = parser.parse_args()
    inputs = {name: source_bytes(getattr(args, name)) for name in ("ecdict", "cedict", "frequency")}
    output, counts = compile_data(**inputs)
    sources = {}
    for name, content in inputs.items():
        path = getattr(args, name)
        sources[name] = {"file": path.name, "sha256": digest(path.read_bytes()), "content_sha256": digest(content)}
    manifest = {"schema": 1, "format": MAGIC.decode("ascii"), "file": args.output.name,
                "bytes": len(output), "sha256": digest(output), "counts": counts, "sources": sources}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(output)
    args.manifest.write_text(json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(manifest, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
