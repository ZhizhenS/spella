#!/usr/bin/env python3
"""Check the reviewed seed offline, or fetch an explicit revision-review proposal.

No search results are automatically admitted. --propose-refresh never changes
the approved JSON, TSV or runtime module. There are no third-party dependencies.
"""
import argparse
import hashlib
import gzip
import json
from pathlib import Path
import re
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
CATALOG = ROOT / "data/dict/modern-entities.json"
RUNTIME = ROOT / "data/dict/modern-entities.tsv"
SNAPSHOT = ROOT / "data/dict/raw/modern-entities/wikidata-snapshot.json.gz"
HEADER = "# EAMODEN1\ttext\taliases\tkind\tdefinition_zh\twikidata_id\trevision\tofficial_url\n"


def image(entries):
    return HEADER + "".join("\t".join([
        entry["text"], ",".join(entry["aliases"]), entry["entity_kind"],
        entry["definition_zh"], entry["wikidata_id"],
        str(entry["wikidata_revision"]), entry["official_url"],
    ]) + "\n" for entry in entries)


def checked(check_runtime=True):
    data = json.loads(CATALOG.read_text(encoding="utf-8"))
    snapshot_bytes = SNAPSHOT.read_bytes()
    snapshot_raw = gzip.decompress(snapshot_bytes)
    snapshot = json.loads(snapshot_raw)
    if data["schema"] != 1 or data["runtime_format"] != "EAMODEN1" or data["data_license"] != "CC0-1.0":
        raise ValueError("Unsupported reviewed entity schema")
    if data["snapshot"]["file"] != "raw/modern-entities/wikidata-snapshot.json.gz" or data["snapshot"]["sha256"] != hashlib.sha256(snapshot_bytes).hexdigest() or data["snapshot"]["content_sha256"] != hashlib.sha256(snapshot_raw).hexdigest():
        raise ValueError("Reviewed Wikidata snapshot digest mismatch")
    entries = data["entries"]
    if not 30 <= len(entries) <= 80:
        raise ValueError("The reviewed seed must contain 30 to 80 entries")
    aliases, ids = set(), set()
    previous = ""
    for entry in entries:
        qid, revision = entry["wikidata_id"], entry["wikidata_revision"]
        if not re.fullmatch(r"Q[1-9][0-9]*", qid) or type(revision) is not int or revision <= 0 or qid in ids:
            raise ValueError("Unverified or duplicate entity identity")
        ids.add(qid)
        source = snapshot["entities"].get(qid)
        if not isinstance(source, dict):
            raise ValueError("Entity identity is absent from the frozen structured snapshot")
        if source["id"] != qid or source["lastrevid"] != revision or source["modified"] != entry["wikidata_modified"]:
            raise ValueError("Entity revision does not match frozen structured data")
        if source["descriptions"]["en"]["value"] != entry["wikidata_description_en"]:
            raise ValueError("English source description changed")
        expected_url = f"https://www.wikidata.org/w/index.php?title={qid}&oldid={revision}"
        if entry["revision_url"] != expected_url or not entry["official_url"].startswith("https://"):
            raise ValueError("Invalid source URL")
        keys = entry["aliases"]
        if not keys or keys != sorted(set(keys)) or keys[0] <= previous:
            raise ValueError("Aliases must be stable and sorted")
        previous = keys[0]
        for key in keys:
            if not re.fullmatch(r"[a-z]{1,64}", key) or key in aliases:
                raise ValueError("Invalid or ambiguous normalized alias")
            aliases.add(key)
        text = entry["text"]
        if not text.isascii() or not 1 <= len(text) <= 96 or re.sub(r"[^a-z]", "", text.lower()) not in keys:
            raise ValueError("Canonical spelling lacks a reviewed alias")
        if not 1 <= len(entry["entity_kind"]) <= 6 or not 1 <= len(entry["definition_zh"]) <= 96:
            raise ValueError("Unbounded entity annotation")
        if any(any(ord(c) < 32 for c in str(entry[field])) for field in ("text", "entity_kind", "definition_zh", "official_url")):
            raise ValueError("Control characters are not allowed in the runtime seed")
    expected = image(entries).encode("utf-8")
    if check_runtime and RUNTIME.read_text(encoding="utf-8").encode("utf-8") != expected:
        raise ValueError("Runtime TSV differs from the reviewed catalog; use --write-runtime only after review")
    return data


def proposal(data, output):
    ids = [entry["wikidata_id"] for entry in data["entries"]]
    entities = {}
    for start in range(0, len(ids), 50):
        query = urllib.parse.urlencode({"action": "wbgetentities", "ids": "|".join(ids[start:start + 50]), "languages": "en|zh|zh-hans|zh-cn", "props": "info|labels|descriptions", "format": "json"})
        request = urllib.request.Request("https://www.wikidata.org/w/api.php?" + query, headers={"User-Agent": "EnglishAssistDictionaryResearch/0.5 (explicit offline-catalog review)"})
        with urllib.request.urlopen(request, timeout=40) as response:
            fetched = json.load(response)
        if "error" in fetched:
            raise ValueError("Wikidata rejected the explicit refresh request")
        entities.update(fetched["entities"])
    result = {"schema": 1, "review_required": True, "note": "Proposal only; manually review identity, official pages, Chinese descriptions and revisions before updating the approved seed.", "entities": entities}
    with output.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Offline-only validation (default)")
    parser.add_argument("--write-runtime", action="store_true", help="Regenerate TSV from already reviewed JSON; no network")
    parser.add_argument("--propose-refresh", type=Path, help="Explicit network refresh to a separate review file, never approved inputs")
    args = parser.parse_args()
    if args.write_runtime:
        data = checked(check_runtime=False)
        RUNTIME.write_text(image(data["entries"]), encoding="utf-8", newline="\n")
    data = checked()
    if args.propose_refresh:
        if args.propose_refresh.resolve() in (CATALOG.resolve(), RUNTIME.resolve(), SNAPSHOT.resolve()):
            raise ValueError("A proposal must not overwrite approved source/runtime files")
        proposal(data, args.propose_refresh)
    print(f"Reviewed modern-entity seed: {len(data['entries'])} verified IDs/revisions; offline inputs consistent")


if __name__ == "__main__":
    main()
