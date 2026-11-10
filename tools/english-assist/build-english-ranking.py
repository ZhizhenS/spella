#!/usr/bin/env python3
"""Offline compiler for the English candidate ranking overlay (EARANK01).

The base vocabulary counts come from Google Books (SymSpell frequency list).
That corpus over-weights archaic and literary words, under-weights spoken and
modern words, stores every contraction with a flat placeholder count, and keeps
personal/place names in lowercase. The pinned vocabulary image stays unchanged;
this overlay replaces individual counts at lookup time.

Rows: word, replacement count, optional display spelling, reason.
  contraction  reviewed spoken-frequency tier (the source has placeholders)
  everyday     reviewed everyday/chat word missing or under-counted
  modern       reviewed modern product word with its canonical casing
  typo         malformed source row, effectively removed
  archaic      reviewed list or an ECDICT archaic marker on the first sense
  name         ECDICT first sense is a personal or place name (demoted)
  proper       ECDICT spells the word only capitalized (Monday, Oklahoma)

--check (default) is read-only; --write regenerates the TSV. Standard library only.
"""
import argparse
import csv
import gzip
import hashlib
import io
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
FREQUENCY = ROOT / "data/dict/raw/english/frequency.txt"
ECDICT = ROOT / "data/dict/raw/english-assist/ecdict.csv.gz"
OUTPUT = ROOT / "data/dict/english-ranking.tsv"
HEADER = "# EARANK01\tword\tfrequency\tdisplay\treason\n"

# Units match frequency.txt (could=302M, hello=33M, okay=15M).
TIER_1, TIER_2, TIER_3 = 150_000_000, 50_000_000, 8_000_000
CONTRACTIONS = {
    TIER_1: "don't i'm it's can't that's didn't i'll you're doesn't i've isn't won't let's there's he's she's we're they're what's wasn't i'd",
    TIER_2: "couldn't wouldn't shouldn't aren't haven't hasn't hadn't weren't you'll we'll they'll he'll she'll it'll you've we've they've you'd he'd she'd we'd they'd here's where's who's how's o'clock could've should've would've might've that'll",
    TIER_3: "needn't mustn't how'd how'll how're it'd somebody's someone's something's who'd who'll who're why'd why're why's",
}
EVERYDAY = {"hi": 30_000_000, "ok": 30_000_000, "covid": 20_000_000, "emoji": 5_000_000, "emojis": 1_500_000, "pls": 2_000_000, "plz": 1_000_000, "thx": 2_000_000, "selfie": 3_000_000, "app": 40_000_000, "apps": 25_000_000}
MODERN = {"iphone": (8_000_000, "iPhone"), "ipad": (3_000_000, "iPad"), "ios": (3_000_000, "iOS"), "macos": (1_000_000, "macOS"), "youtube": (10_000_000, "YouTube"), "github": (2_000_000, "GitHub"), "linkedin": (2_000_000, "LinkedIn"), "paypal": (2_000_000, "PayPal"), "tiktok": (2_000_000, "TikTok"), "whatsapp": (2_000_000, "WhatsApp"), "wechat": (1_000_000, "WeChat"), "javascript": (3_000_000, "JavaScript")}
TYPOS = {"you'v"}
ARCHAIC = set("""
thee thou thy thine thyself ye hath doth dost hast hadst shalt shan't wilt wouldst couldst shouldst canst mayst mightst
didst wert unto ere oft ofttimes oftener hither thither whither whence thence wherefore methinks forsooth
verily prithee perchance betwixt nigh yore wonted twas mayhap haply peradventure begone fie hark
""".split())
# Formal but current words (hitherto, thereof, whereupon) are deliberately absent.
# ECDICT usage labels only; "[古生]" (paleontology) and etymology notes are not archaic.
ARCHAIC_MARKER = re.compile(r"<古>|<废>|\[古\]|\[废\]|\[古语[^\]]*\]|\[古英语\]")
NOT_ARCHAIC = {"hyping", "quo", "anon"}  # ECDICT attaches an obsolete homograph's sense
NOT_NAME = {"foo", "len"}  # programming words that ECDICT lists only as surnames
# Reviewed: ordinary lowercase words that ECDICT happens to store capitalized.
NOT_PROPER = set("""
ladies demo logos conservative creator val sellers liberal labour labor democrat democrats republican
republicans catholic protestant gothic baroque renaissance victorian bible gospel scripture heaven hell
web internet earth sun moon pole crescent utopia staples titanic federated transformers regents surfers
booksellers arg dis aus prof mus khz lett chr obs zine wac prov hama kos tyr mach
""".split())
NAME_MARKER = re.compile(r"[（(][^）)]*(?:男子名|女子名|男名|女名|人名|姓氏|地名|州名|国名|城市名|市名|岛名)[^）)]*[）)]|[（(][^）)]*(?:城市|城镇|镇|市|州|郡|县|岛|首府|首都)[）)]")
INFLECTIONS = ("s", "es", "ed", "d", "er", "r", "ing")
ARCHAIC_DIVISOR, NAME_DIVISOR = 200, 3


def frequencies():
    counts = {}
    for line in FREQUENCY.read_text(encoding="utf-8-sig").splitlines():
        word, _, count = line.strip().rpartition(" ")
        if word and count.isdigit():
            counts[word] = counts.get(word, 0) + int(count)
    return counts


def first_sense(translation):
    line = translation.replace("\\n", "\n").split("\n", 1)[0]
    return re.split(r"[；;,，]", line, maxsplit=1)[0]


def inflected(word, counts):
    """ECDICT gives some inflections only a surname sense (beers, abler)."""
    return any(word.endswith(end) and counts.get(word[: -len(end)], 0) >= counts[word] for end in INFLECTIONS)


def ecdict_rows():
    with gzip.open(ECDICT, "rt", encoding="utf-8", newline="") as stream:
        reader = csv.reader(stream)
        header = next(reader)
        word, translation = header.index("word"), header.index("translation")
        for row in reader:
            yield row[word], row[translation]


def compile_rows():
    counts = frequencies()
    rows = {}
    for tier, words in CONTRACTIONS.items():
        for word in words.split():
            display = "I" + word[1:] if word.startswith("i'") else ""
            rows[word] = (tier, display, "contraction")
    for word, count in EVERYDAY.items():
        rows[word] = (max(count, counts.get(word, 0)), "", "everyday")
    for word, (count, display) in MODERN.items():
        rows[word] = (max(count, counts.get(word, 0)), display, "modern")
    for word in TYPOS:
        rows[word] = (1, "", "typo")
    archaic, names, capitalized = set(ARCHAIC), {}, {}
    lowercase_seen = set()
    for word, translation in ecdict_rows():
        key = word.lower()
        if key not in counts or key in rows:
            continue
        sense = first_sense(translation)
        if word == key:
            lowercase_seen.add(key)
            if ARCHAIC_MARKER.search(sense) and key not in NOT_ARCHAIC:
                archaic.add(key)
        elif word[:1].isupper() and not word.isupper() and word.isascii():
            capitalized.setdefault(key, word)
        name_sense = sense.lstrip().startswith("n.") or NAME_MARKER.match(sense.lstrip())
        if name_sense and NAME_MARKER.search(sense) and key not in NOT_NAME and not inflected(key, counts):
            names.setdefault(key, word[:1].upper() + word[1:])
    for key in sorted(archaic):
        if key in counts and key not in rows:
            rows[key] = (max(1, counts[key] // ARCHAIC_DIVISOR), "", "archaic")
    for key, display in sorted(names.items()):
        if key not in rows and display.lower() == key:
            rows[key] = (max(1, counts[key] // NAME_DIVISOR), display, "name")
    # ECDICT stores proper nouns (Monday, English, Oklahoma) only capitalized;
    # those keep their count and gain their written casing.
    for key, display in sorted(capitalized.items()):
        if (key not in rows and key not in lowercase_seen and key not in NOT_NAME | NOT_PROPER
                and display == key.capitalize() and not inflected(key, counts)):
            rows[key] = (counts[key], display, "proper")
    return rows


def image(rows):
    body = "".join(f"{word}\t{count}\t{display}\t{reason}\n" for word, (count, display, reason) in sorted(rows.items()))
    return (HEADER + body).encode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="Read-only comparison (default)")
    parser.add_argument("--write", action="store_true", help="Regenerate the runtime TSV")
    args = parser.parse_args()
    if args.check and args.write:
        parser.error("--check cannot be combined with --write")
    data = image(compile_rows())
    if args.write:
        OUTPUT.write_bytes(data)
    elif OUTPUT.read_bytes() != data:
        raise SystemExit("english-ranking.tsv differs from its deterministic compilation; run --write after review")
    reasons = {}
    for line in data.decode("utf-8").splitlines()[1:]:
        reasons[line.rsplit("\t", 1)[1]] = reasons.get(line.rsplit("\t", 1)[1], 0) + 1
    print(f"EARANK01 {len(data)} bytes sha256={hashlib.sha256(data).hexdigest()} rows={dict(sorted(reasons.items()))}")


if __name__ == "__main__":
    main()
