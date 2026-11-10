#!/usr/bin/env python3
"""Freeze Kaikki raw-English modern senses, then compile the reviewed data offline.

Normal --check never downloads or changes files. Network and source replacement
are explicit --download-source / --extract-full operations. Dictionary text is
CC BY-SA 4.0 (not the MIT licence of the separate Wiktextract program).
"""
import argparse
import concurrent.futures
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import time
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
RAW = ROOT / "data/dict/raw/kaikki-modern"
TSV = ROOT / "data/dict/modern-lexicon.tsv"
MANIFEST = ROOT / "data/dict/modern-lexicon.json"
EDITORIAL = ROOT / "data/dict/modern-lexicon-zh-overrides.json"
SOURCE_URL = "https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz"
SOURCE_PAGE = "https://kaikki.org/dictionary/rawdata.html"
ZH_SOURCE_URL = "https://kaikki.org/dictionary/downloads/zh/zh-extract.jsonl.gz"
HEADER = "# EAMODLX1\tkey\ttext\tpos\tdefinition_zh\tusage\tfull_form\tsource_url\tsense_id\tpriority\n"
POS = {"noun": "名词", "verb": "动词", "adj": "形容词", "adv": "副词", "phrase": "短语", "prep_phrase": "介词短语", "intj": "感叹词", "pron": "代词", "conj": "连词", "prep": "介词", "name": "专名", "num": "数词", "det": "限定词", "contraction": "缩约词", "particle": "小品词", "symbol": "符号"}
USAGE = {"词汇", "网络", "缩写", "俚语", "非正式", "专业", "旧义", "粗俗"}
# Register policy for a general-purpose input method. Offensive and archaic
# senses never enter the runtime catalog; vulgar or sexually explicit senses
# stay available only when their exact spelling is typed (runtime "粗俗").
# "derogatory" alone marks mild pejoratives (noob, cringe, stan) and is kept.
OFFENSIVE_TAGS = {"offensive", "slur", "ethnic"}
OBSOLETE_TAGS = {"archaic", "obsolete"}
OFFENSIVE_RAW = re.compile(r"冒犯|歧[視视]|slur|offens", re.I)
OBSOLETE_RAW = re.compile(r"古舊|古旧")
DATED_RAW = re.compile(r"過時|过时|罕用")
VULGAR_DEFINITION = re.compile(r"性交|性行[為为]|做[愛爱]|[陰阴][莖茎道戶户]|射精|性高潮|妓|婊|屌|屄|肏|淫|肛|手淫|自慰|口交|奶子|[睪睾]丸|生殖器|乳房(?!炎)|榨精|[內内]射|中出|[陽阳]具")
VULGAR_WORD = re.compile(r"fuck|shit|cunt|dick|cock|pussy|bitch|whore|slut|wank|twat|bollock|bastard", re.I)
# Reviewed: untagged slurs in the frozen source.
EXCLUDED_WORDS = {"tran"}
MODERN_TAGS = {"slang", "Internet", "abbreviation", "initialism", "acronym", "informal", "neologism"}
MODERN_CATEGORIES = {"English slang", "English internet slang", "English text messaging slang", "English abbreviations", "English initialisms", "English acronyms", "English informal terms", "English neologisms"}
ZH_MODERN_CATEGORIES = {"英語俚語", "英語網路用語", "英語網絡用語", "英語簡訊俚語", "英語縮寫", "英語首字母縮略詞", "英語首字母缩略词", "英語首字母縮拼詞", "英語首字母縮略字", "英語非正式用語", "英語新詞", "英語縮略形式"}
ABBR_CATEGORIES = {"English abbreviations", "English initialisms", "English acronyms", "英語縮寫", "英語首字母縮略詞", "英語首字母缩略词", "英語首字母縮拼詞", "英語首字母縮略字", "英語縮略形式"}
REVIEW_WORDS = set("idk imo imho tbh ngl brb afk lol smh irl btw fyi asap aka gg goat fomo yolo tldr dm pov icymi afaik ikr rizz sus doomscrolling ghosting delulu yeet lmao rofl omg wtf wth ftw nvm rn ily lmk np noob newbie bae based cringe cooked stan simp ratio fwiw nsfw sfw tmi diy ootd mid vibe vibes slay salty lowkey highkey".split())
SENSE_FIELDS = ("glosses", "raw_glosses", "tags", "raw_tags", "categories", "alt_of", "form_of", "senseid")
ZH_CODES = {"zh", "cmn", "zh-hans", "zh-hant"}


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        while block := f.read(4 * 1024 * 1024):
            h.update(block)
    return h.hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes((json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))


def norm(value):
    return re.sub(r"\s+", " ", value).strip().rstrip(".").casefold()


def input_key(text):
    return re.sub(r"[^a-z]", "", text.lower())


def selected(sense):
    return bool(set(sense.get("tags", ())) & MODERN_TAGS or set(sense.get("categories", ())) & MODERN_CATEGORIES)


def translations(items):
    fields = ("code", "lang_code", "lang", "word", "sense", "roman", "tags", "raw_tags")
    return [{k: item[k] for k in fields if k in item} for item in items if item.get("code", item.get("lang_code")) in ZH_CODES]


def project(entry, line, edition="en"):
    """Only lexical fields: no examples, quotations, pronunciations or audio."""
    if entry.get("lang_code") != "en":
        return None
    word = entry.get("word", "")
    if not isinstance(word, str) or not word.isascii() or not 1 <= len(word) <= 96 or any(ord(c) < 32 or ord(c) == 127 for c in word) or not re.fullmatch(r"[a-z]{2,64}", input_key(word)):
        return None
    all_senses = entry.get("senses", [])
    senses = []
    # Entry-level translation labels may identify a parent gloss shared by many
    # senses. Such labels are ambiguous and are deliberately not copied to all.
    claims = {}
    for index, sense in enumerate(all_senses):
        for gloss in set(map(norm, sense.get("glosses", []))):
            claims.setdefault(gloss, set()).add(index)
    entry_translations = translations(entry.get("translations", []))
    for index, original in enumerate(all_senses):
        entry_modern = bool(set(entry.get("categories", [])) & (ZH_MODERN_CATEGORIES if edition == "zh" else MODERN_CATEGORIES))
        if not selected(original) and not (edition == "zh" and bool(set(original.get("categories", [])) & ZH_MODERN_CATEGORIES)) and not entry_modern and input_key(word) not in REVIEW_WORDS:
            continue
        sense = {k: original[k] for k in SENSE_FIELDS if k in original}
        identity = {"word": word, "pos": entry.get("pos"), "etymology_number": entry.get("etymology_number"), "sense": sense}
        sense["sense_id"] = edition + "-" + digest(canonical(identity))[:40]
        sense["ordinal"] = index
        matched = translations(original.get("translations", []))
        for tr in entry_translations:
            label = norm(tr.get("sense", ""))
            if label and claims.get(label) == {index}:
                matched.append(tr)
        sense["translations_zh"] = sorted({canonical(t): t for t in matched}.values(), key=canonical)
        senses.append(sense)
    if not senses:
        return None
    return {"edition": edition, "source_line": line, "word": word, "pos": entry.get("pos", ""), "etymology_number": entry.get("etymology_number"), "senses": senses}


def download(directory, edition="en"):
    """Explicit full-source fetch, resumable HTTP range chunks, no API keys."""
    directory.mkdir(parents=True, exist_ok=True)
    page = urllib.request.urlopen(SOURCE_PAGE, timeout=60).read()
    url = SOURCE_URL if edition == "en" else ZH_SOURCE_URL
    expected_link = b'href="raw-wiktextract-data.jsonl.gz"' if edition == "en" else b'zh-extract.jsonl.gz'
    if expected_link not in page:
        raise ValueError("Official raw download link changed; review it first")
    with urllib.request.urlopen(urllib.request.Request(url, method="HEAD"), timeout=60) as response:
        headers = dict(response.headers)
    size, etag = int(headers["Content-Length"]), headers["ETag"]
    block = 64 * 1024 * 1024
    def fetch(index):
        start, end = index * block, min(size - 1, (index + 1) * block - 1)
        target = directory / f"part-{index:03}.bin"
        if target.exists() and target.stat().st_size == end - start + 1:
            return target
        request = urllib.request.Request(url, headers={"Range": f"bytes={start}-{end}", "If-Range": etag, "User-Agent": "EnglishAssist-reviewed-dictionary/0.6"})
        with urllib.request.urlopen(request, timeout=180) as response:
            if response.status != 206 or response.headers.get("Content-Range") != f"bytes {start}-{end}/{size}":
                raise ValueError("Source changed during ranged download")
            with target.with_suffix(".partial").open("wb") as out:
                while data := response.read(1024 * 1024):
                    out.write(data)
        if target.with_suffix(".partial").stat().st_size != end - start + 1:
            raise ValueError("Truncated source chunk")
        target.with_suffix(".partial").replace(target)
        return target
    # Never reuse chunks from another ETag/source version.
    stamp = {"url": url, "etag": etag, "bytes": size}
    stamp_path = directory / "range-version.json"
    if stamp_path.exists() and json.loads(stamp_path.read_text()) != stamp:
        raise ValueError("Cache contains a different source version; use a new directory")
    write_json(stamp_path, stamp)
    count = (size + block - 1) // block
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        parts = list(pool.map(fetch, range(count)))
    target = directory / ("raw-wiktextract-data.jsonl.gz" if edition == "en" else "zh-extract.jsonl.gz")
    with target.with_suffix(".partial").open("wb") as out:
        for part in parts:
            with part.open("rb") as source:
                while data := source.read(4 * 1024 * 1024):
                    out.write(data)
    target.with_suffix(".partial").replace(target)
    meta = {"url": url, "bytes": size, "sha256": file_digest(target), "etag": etag, "last_modified": headers["Last-Modified"], "complete": True}
    write_json(directory / "download.json", meta)
    (directory / "rawdata.html").write_bytes(page)
    print(json.dumps(meta), flush=True)


def extract(source, metadata, output, edition="en"):
    meta = json.loads(metadata.read_text(encoding="utf-8-sig"))
    meta = {key: meta[key] for key in ("url", "bytes", "sha256", "etag", "last_modified", "complete")}
    if meta["url"] != (SOURCE_URL if edition == "en" else ZH_SOURCE_URL) or not meta.get("complete") or source.stat().st_size != meta["bytes"] or file_digest(source) != meta["sha256"]:
        raise ValueError("Complete raw source digest/metadata mismatch")
    output.mkdir(parents=True, exist_ok=True)
    stats = {"source_lines": 0, "english_entries": 0, "selected_entries": 0, "selected_senses": 0}
    raw_digest = hashlib.sha256()
    projected = io.BytesIO()
    with gzip.open(source, "rb") as inp:
        for line, data in enumerate(inp, 1):
            raw_digest.update(data)
            stats["source_lines"] = line
            if b'"lang_code": "en"' not in data and b'"lang_code":"en"' not in data:
                continue
            entry = json.loads(data)
            if entry.get("lang_code") != "en":
                continue
            stats["english_entries"] += 1
            item = project(entry, line, edition)
            if item:
                item["source_entry_sha256"] = digest(data)
                projected.write(canonical(item) + b"\n")
                stats["selected_entries"] += 1
                stats["selected_senses"] += len(item["senses"])
            if line % 100000 == 0:
                print(json.dumps(stats), flush=True)
    content = projected.getvalue()
    target = output / ("english-modern-senses.jsonl.gz" if edition == "en" else "chinese-edition-modern-senses.jsonl.gz")
    with target.open("wb") as out:
        with gzip.GzipFile(filename="", mode="wb", fileobj=out, mtime=0, compresslevel=9) as zipped:
            zipped.write(content)
    manifest = {"schema": 1, "edition": edition, "data_license": "CC-BY-SA-4.0", "alternative_upstream_license": "GFDL-1.3-or-later", "source_page": SOURCE_PAGE, "source": meta | {"content_sha256": raw_digest.hexdigest()}, "extraction": {"version": 1, "language": "en", "word_rule": "Printable ASCII text 1..96; key removes nonletters, resulting [a-z]{2,64}", "tags_any": sorted(MODERN_TAGS), "categories_any": sorted(MODERN_CATEGORIES | (ZH_MODERN_CATEGORIES if edition == "zh" else set())), "review_words": sorted(REVIEW_WORDS), "projection": "Lexical fields only; examples/quotations/audio excluded. Entry-level Chinese translations require a unique exact gloss match. All senses of explicitly reviewed seed words retained without inventing upstream usage labels.", **stats}, "subset": {"file": target.name, "bytes": target.stat().st_size, "sha256": file_digest(target), "content_bytes": len(content), "content_sha256": digest(content)}}
    write_json(output / ("SOURCES.json" if edition == "en" else "ZH-SOURCES.json"), manifest)
    print(json.dumps(manifest, ensure_ascii=False), flush=True)


def source_records(raw_dir, edition="en"):
    meta = json.loads((raw_dir / ("SOURCES.json" if edition == "en" else "ZH-SOURCES.json")).read_text(encoding="utf-8"))
    expected_file = "english-modern-senses.jsonl.gz" if edition == "en" else "chinese-edition-modern-senses.jsonl.gz"
    if meta.get("schema") != 1 or meta.get("data_license") != "CC-BY-SA-4.0" or meta["subset"]["file"] != expected_file:
        raise ValueError("Unsupported frozen source schema")
    compressed = (raw_dir / meta["subset"]["file"]).read_bytes()
    if len(compressed) != meta["subset"]["bytes"] or digest(compressed) != meta["subset"]["sha256"]:
        raise ValueError("Frozen subset compressed digest mismatch")
    content = gzip.decompress(compressed)
    if len(content) != meta["subset"]["content_bytes"] or digest(content) != meta["subset"]["content_sha256"]:
        raise ValueError("Frozen subset content digest mismatch")
    records = [json.loads(line) for line in content.splitlines()]
    senses = sum(len(r["senses"]) for r in records)
    if len(records) != meta["extraction"]["selected_entries"] or senses != meta["extraction"]["selected_senses"]:
        raise ValueError("Frozen source counts mismatch")
    return meta, records


def has_hanzi(text):
    return any("\u3400" <= c <= "\u9fff" for c in text)


def meaningful_chinese(text):
    if not has_hanzi(text) or norm(text) in {"网络用语", "網路用語", "网络流行语", "縮寫", "缩写", "俚语", "俚語", "非正式用语", "詞彙", "词汇"}:
        return False
    if re.search(r"(?i)template:|lua error|<ref|模板[:：]", text):
        return False
    if re.fullmatch(r"[\x20-\x7e]+的[過过]去(?:式|分[詞词])", text.replace(" ", "")):
        return False
    return True


def clean_field(text, maximum, ascii_only=False):
    if not isinstance(text, str) or len(text) > maximum or any(ord(c) < 32 or ord(c) == 127 for c in text) or (ascii_only and not text.isascii()):
        raise ValueError("Invalid/bounded runtime field")
    return text


def register(word, sense, definition):
    """Return "offensive", "obsolete", "vulgar" or "" for one admitted sense."""
    tags, raw = set(sense.get("tags", [])), " ".join(sense.get("raw_tags", []))
    if word.lower() in EXCLUDED_WORDS or tags & OFFENSIVE_TAGS or OFFENSIVE_RAW.search(raw):
        return "offensive"
    if tags & OBSOLETE_TAGS or OBSOLETE_RAW.search(raw):
        return "obsolete"
    vulgar_raw = "粗" in raw.replace("不粗", "")
    if "vulgar" in tags or vulgar_raw or VULGAR_DEFINITION.search(definition) or VULGAR_WORD.search(word):
        return "vulgar"
    return ""


def usage(sense):
    tags, categories = set(sense.get("tags", [])), set(sense.get("categories", []))
    if tags & {"archaic", "dated", "obsolete", "historical"} or DATED_RAW.search(" ".join(sense.get("raw_tags", []))):
        return "旧义"
    if tags & {"abbreviation", "initialism", "acronym"} or categories & ABBR_CATEGORIES:
        return "缩写"
    if "Internet" in tags or categories & {"English internet slang", "English text messaging slang", "英語網路用語", "英語網絡用語", "英語簡訊俚語"}:
        return "网络"
    if "slang" in tags:
        return "俚语"
    if "informal" in tags:
        return "非正式"
    return "词汇"


def full_form(sense):
    links = sense.get("alt_of", [])
    if not links and (set(sense.get("categories", [])) & ABBR_CATEGORIES or set(sense.get("tags", [])) & {"abbreviation", "initialism", "acronym"}):
        links = sense.get("form_of", [])
    if len(links) == 1:
        value = links[0].get("word", "")
        if len(value) <= 256 and value == value.strip() and all(ord(c) >= 32 and ord(c) != 127 for c in value):
            return value
    return ""


def chinese_gloss(sense):
    """Keep genuine Chinese meaning; relation-only Chinese is not a translation."""
    for gloss in reversed(sense.get("glosses", [])):
        # Some source glosses incorrectly embed an example on later lines;
        # retain only the first definition line, never that example text.
        gloss = gloss.split("\n", 1)[0].strip()
        if not has_hanzi(gloss) or len(gloss) > 160:
            continue
        relation = set(sense.get("tags", [])) & {"alt-of", "form-of"} or re.search(r"(?:大小[寫写]|替代|[變变][體体]|[寫写]法|拼[寫写]|[縮缩][寫写略]|首字母|過去|过去|分詞|分词|複數|复数)", gloss)
        if relation:
            quoted = re.findall(r"[“「]([^”」]+)[”」]", gloss)
            meaningful = [text for text in quoted if has_hanzi(text)]
            if meaningful:
                return "；".join(meaningful)[:160]
            prefix = re.match(r"^([^（(]+)[（(]", gloss)
            if prefix and has_hanzi(prefix[1]) and not re.search(r"[縮缩][寫写略]|[寫写]法|形式|替代", prefix[1]):
                return prefix[1].strip()
            tail = re.split(r"[:：]", gloss, maxsplit=1)
            if len(tail) == 2 and has_hanzi(tail[1]):
                return tail[1].strip()
            continue
        return gloss
    return ""


def compile_records(records, editorial):
    if editorial.get("schema") != 1 or editorial.get("definition_origin") != "project-editorial":
        raise ValueError("Unsupported editorial Chinese schema")
    overrides = {}
    for item in editorial["entries"]:
        sid = item["sense_id"]
        if sid in overrides:
            raise ValueError("Duplicate editorial sense override")
        overrides[sid] = item
    seen, rows, audit = set(), [], []
    counts = {"upstream_chinese_senses": 0, "editorial_chinese_senses": 0, "missing_chinese_senses": 0, "unsupported_pos_senses": 0, "duplicate_senses": 0, "relation_only_chinese_senses": 0, "invalid_lexical_chinese_senses": 0, "excluded_offensive_senses": 0, "excluded_obsolete_senses": 0, "vulgar_exact_only_senses": 0}
    for record in records:
        text, source_pos = record["word"], record["pos"]
        key = input_key(text)
        if not re.fullmatch(r"[a-z]{2,64}", key) or not text.isascii() or not 1 <= len(text) <= 96:
            raise ValueError("Invalid frozen word")
        clean_field(text, 96, True)
        if type(record["source_line"]) is not int or record["source_line"] < 1 or not re.fullmatch(r"[0-9a-f]{64}", record["source_entry_sha256"]):
            raise ValueError("Invalid frozen source line evidence")
        for sense in record["senses"]:
            sid = sense["sense_id"]
            if not re.fullmatch(r"(?:en|zh)-[0-9a-f]{40}", sid):
                raise ValueError("Invalid stable sense identity")
            if sid in seen:
                counts["duplicate_senses"] += 1
                continue
            seen.add(sid)
            if source_pos not in POS:
                counts["unsupported_pos_senses"] += 1
                continue
            words = [tr.get("word", "") for tr in sense.get("translations_zh", [])]
            words = sorted(set(w for w in words if has_hanzi(w) and len(w) <= 80 and not any(ord(c) < 32 for c in w)))
            definition = chinese_gloss(sense) if record.get("edition") == "zh" else "；".join(words)
            if record.get("edition") == "zh" and not definition and any(has_hanzi(g) for g in sense.get("glosses", [])):
                counts["relation_only_chinese_senses"] += 1
            if len(definition) > 160:
                definition = "；".join(words[:1])
            origin = "upstream-chinese-edition-gloss" if record.get("edition") == "zh" else "upstream-sense-translation"
            source_definition = definition
            override = overrides.get(sid)
            if override:
                if override["word"] != text or override["pos"] != source_pos or override["glosses"] != sense.get("glosses", []) or override.get("full_form", "") != full_form(sense):
                    raise ValueError("Editorial Chinese no longer matches its exact upstream sense")
                definition = override["definition_zh"]
                origin = "project-editorial"
                if override.get("definition_from_upstream_chinese"):
                    if record.get("edition") != "zh" or definition != source_definition:
                        raise ValueError("Source-Chinese attribution requires the unchanged native definition")
                    origin = "upstream-chinese-edition-gloss"
                clean_field(definition, 160)
                if not meaningful_chinese(definition):
                    raise ValueError("Editorial definition must contain an actual Chinese meaning")
            if not definition:
                counts["missing_chinese_senses"] += 1
                continue
            if not meaningful_chinese(definition):
                counts["invalid_lexical_chinese_senses"] += 1
                continue
            clean_field(definition, 160)
            if not has_hanzi(definition):
                raise ValueError("Every admitted sense must have a real Chinese definition")
            tags = set(sense.get("tags", []))
            policy = register(text, sense, definition)
            if policy in ("offensive", "obsolete"):
                counts[f"excluded_{policy}_senses"] += 1
                continue
            priority = 500
            priority -= 300 if tags & {"archaic", "dated", "obsolete", "historical"} else 0
            priority -= 150 if tags & {"rare", "uncommon", "nonstandard"} else 0
            priority -= 100 if any(t in {"Scotland", "Ireland", "Australia", "New-Zealand", "Multicultural-London-English", "dialectal", "regional"} for t in tags) else 0
            if override:
                priority = override.get("priority", priority)
            if type(priority) is not int or not 0 <= priority <= 0xffffffff:
                raise ValueError("Invalid priority")
            url = ("https://zh.wiktionary.org/wiki/" if record.get("edition") == "zh" else "https://en.wiktionary.org/wiki/") + urllib.parse.quote(text, safe="") + ("#英語" if record.get("edition") == "zh" else "#English")
            label = override.get("usage", usage(sense)) if override else usage(sense)
            if policy == "vulgar":
                label = "粗俗"
                counts["vulgar_exact_only_senses"] += 1
            if label not in USAGE:
                raise ValueError("Unknown usage label")
            display_form = full_form(sense)
            if override and "display_full_form" in override:
                display_form = clean_field(override["display_full_form"], 256)
                if not display_form or display_form != display_form.strip() or not any(display_form in gloss for gloss in sense.get("glosses", [])):
                    raise ValueError("Editorial expansion lacks literal evidence in the frozen gloss")
            row = [key, text, POS[source_pos], definition, label, display_form, url, sid, str(priority)]
            for value in row:
                clean_field(value, 512)
            rows.append(row)
            counts["editorial_chinese_senses" if origin == "project-editorial" else "upstream_chinese_senses"] += 1
            audit.append({"sense_id": sid, "edition": record.get("edition", "en"), "word": text, "source_line": record["source_line"], "source_entry_sha256": record["source_entry_sha256"], "source_url": url, "pos": source_pos, "glosses": sense.get("glosses", []), "tags": sense.get("tags", []), "full_form": display_form, "definition_zh": definition, "definition_origin": origin, "usage": label, "usage_origin": "project-editorial" if override and "usage" in override else "upstream-tags-or-neutral", "priority": priority})
    missing = set(overrides) - seen
    if missing:
        raise ValueError("Editorial override points to absent upstream sense")
    # Only explicit same-expansion, same-POS abbreviation senses are merged.
    # Native Chinese wins the wording, reviewed priority remains attached to
    # that proven semantic relation. Homographs and unknown relations survive.
    merged = {}
    for row, evidence in zip(rows, audit):
        full = evidence["full_form"]
        identity = (row[0], evidence["pos"], norm(full)) if full and evidence["usage"] == "缩写" else ("sense", evidence["sense_id"])
        old = merged.get(identity)
        if old:
            choose_new = evidence["edition"] == "zh" and old[1]["edition"] != "zh"
            chosen = (row, evidence) if choose_new else old
            other = old[1] if choose_new else evidence
            chosen[1].setdefault("equivalent_sources", []).append(other)
            best_priority = max(int(old[0][-1]), int(row[-1]))
            chosen[0][-1] = str(best_priority)
            chosen[1]["priority"] = best_priority
            merged[identity] = chosen
        else:
            merged[identity] = (row, evidence)
    before_merge = len(rows)
    rows = [pair[0] for pair in merged.values()]
    audit = [pair[1] for pair in merged.values()]
    counts["admitted_senses_before_merge"] = before_merge
    counts["cross_source_merged_senses"] = before_merge - len(rows)
    rows.sort(key=lambda r: (r[0], -int(r[-1]), r[-2]))
    audit.sort(key=lambda r: (r["word"].lower(), -r["priority"], r["sense_id"]))
    image = (HEADER + "".join("\t".join(row) + "\n" for row in rows)).encode("utf-8")
    return image, counts | {"entries": len(rows), "senses": len(rows), "distinct_keys": len({r[0] for r in rows}), "runtime_senses": len(rows), "runtime_keys": len({r[0] for r in rows})}, audit


def build(raw_dir=RAW, editorial_path=EDITORIAL, runtime=TSV, manifest_path=MANIFEST, write=False, refresh=False):
    source, records = source_records(raw_dir)
    zh_source, zh_records = source_records(raw_dir, "zh")
    records += zh_records
    editorial = json.loads(editorial_path.read_text(encoding="utf-8"))
    image, counts, audit = compile_records(records, editorial)
    inputs = {path.relative_to(ROOT).as_posix(): {"bytes": path.stat().st_size, "sha256": file_digest(path)} for path in (raw_dir / "SOURCES.json", raw_dir / "english-modern-senses.jsonl.gz", raw_dir / "ZH-SOURCES.json", raw_dir / "chinese-edition-modern-senses.jsonl.gz", raw_dir / "LICENSE-CC-BY-SA-4.0.html", raw_dir / "NOTICE.txt", editorial_path)}
    expected = {"schema": 1, "format": "EAMODLX1", "runtime_format": "EAMODLX1", "producerSha256": file_digest(Path(__file__)), "file": runtime.name, "bytes": len(image), "sha256": digest(image), "data_license": "CC-BY-SA-4.0", "source_sha256": source["source"]["sha256"], "chinese_source_sha256": zh_source["source"]["sha256"], "counts": counts, "inputs": inputs, "bounds": {"key_ascii_letters": [2, 64], "text_ascii": 96, "definition_unicode": 160, "full_form_unicode": 256, "source_url_bytes": 512, "sense_id": 128}, "entries": audit}
    if refresh:
        write_json(manifest_path, expected)
    elif json.loads(manifest_path.read_text(encoding="utf-8")) != expected:
        raise ValueError("Reviewed modern lexicon manifest differs; explicit review/refresh required")
    if write:
        runtime.write_bytes(image)
    elif runtime.read_bytes() != image:
        raise ValueError("Modern lexicon TSV differs from deterministic reviewed compilation")
    return {"status": "ok", "format": "EAMODLX1", "bytes": len(image), "sha256": digest(image), "counts": counts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write-runtime", action="store_true")
    parser.add_argument("--refresh-manifest", action="store_true")
    parser.add_argument("--download-source", type=Path)
    parser.add_argument("--extract-full", type=Path)
    parser.add_argument("--edition", choices=("en", "zh"), default="en")
    parser.add_argument("--download-metadata", type=Path)
    parser.add_argument("--snapshot-output", type=Path, default=RAW)
    args = parser.parse_args()
    if args.check and (args.download_source or args.extract_full or args.write_runtime or args.refresh_manifest):
        parser.error("--check cannot download, extract, write, or refresh")
    if args.download_source:
        download(args.download_source, args.edition)
    elif args.extract_full:
        if not args.download_metadata:
            parser.error("--extract-full requires --download-metadata from a complete source download")
        extract(args.extract_full, args.download_metadata, args.snapshot_output, args.edition)
    else:
        if args.refresh_manifest and not args.write_runtime:
            parser.error("Review refresh requires --write-runtime")
        print(json.dumps(build(write=args.write_runtime, refresh=args.refresh_manifest), ensure_ascii=False))


if __name__ == "__main__":
    main()
