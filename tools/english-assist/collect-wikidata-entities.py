"""Explicit bounded public Wikidata acquisition to a separate research directory.
Normal builds never call this tool. Queries, exact responses and digests are cached
for later offline modern-entities.py --import-research proposal/review.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = None  # Explicit authoring output only; never an approved input directory.
REPO = Path(__file__).resolve().parents[2]
UA = "EnglishAssistDictionaryResearch/0.6 (read-only bounded offline dictionary import)"
CATEGORIES = {
    "programming-languages": ("Q9143", 600, 2),
    "software": ("Q7397", 900, 4),
    "companies": ("Q783794", 900, 15),
}


def request(url, accept):
    for attempt in range(5):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": accept, "Accept-Encoding": "gzip"})
            with urllib.request.urlopen(req, timeout=45) as response:
                raw = response.read(32 * 1024 * 1024 + 1)
                if len(raw) > 32 * 1024 * 1024:
                    raise ValueError("Response exceeded the research bound")
                if response.headers.get("Content-Encoding") == "gzip":
                    raw = gzip.decompress(raw)
            data = json.loads(raw)
            if "error" in data:
                if data["error"].get("code") != "maxlag":
                    raise ValueError(data["error"].get("code"))
                raise RuntimeError("maxlag")
            return raw, data
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, RuntimeError) as error:
            if isinstance(error, urllib.error.HTTPError) and error.code not in (429, 502, 503, 504):
                raise
            if attempt == 4:
                raise
            retry = error.headers.get("Retry-After", "") if isinstance(error, urllib.error.HTTPError) else ""
            delay = min(60, max(2 ** (attempt + 1), int(retry) if retry.isdigit() else 0))
            print("retry", type(error).__name__, "after", delay, flush=True)
            time.sleep(delay)


def discover():
    rows = {}
    for category, (qid, limit, minimum) in CATEGORIES.items():
        target = ROOT / f"discover-{category}.json"
        relation = f'?item wdt:P31/wdt:P279* wd:{qid} .' if category in ('programming-languages', 'software') else f'?item wdt:P31 wd:{qid} .'
        query = f'''SELECT DISTINCT ?item ?links WHERE {{
 {relation}
 ?item wikibase:sitelinks ?links . FILTER(?links >= {minimum})
 ?item rdfs:label ?en . FILTER(LANG(?en) = "en")
 FILTER EXISTS {{ ?item rdfs:label ?zh . FILTER(LANG(?zh) IN ("zh", "zh-hans", "zh-cn")) }}
}} ORDER BY DESC(?links) ?item LIMIT {limit}'''
        (ROOT / f"discover-{category}.sparql").write_text(query, encoding="utf-8", newline="\n")
        if target.exists():
            data = json.loads(target.read_bytes())
        else:
            raw, data = request("https://query.wikidata.org/sparql?" + urllib.parse.urlencode({"query": query, "format": "json"}), "application/sparql-results+json")
            target.write_bytes(raw)
        bindings = data["results"]["bindings"]
        if len(bindings) > limit:
            raise ValueError("Discovery bound was exceeded")
        for binding in bindings:
            entity_id = binding["item"]["value"].rsplit("/", 1)[-1]
            record = rows.setdefault(entity_id, {"id": entity_id, "categories": [], "discovery_sitelinks": 0})
            record["categories"].append(category)
            record["discovery_sitelinks"] = max(record["discovery_sitelinks"], int(binding["links"]["value"]))
        print(category, len(bindings), "unique so far", len(rows), flush=True)
        time.sleep(1)
    approved = json.loads((REPO / "data/dict/modern-entities.json").read_text(encoding="utf-8-sig"))
    for entry in approved["entries"]:
        record = rows.setdefault(entry["wikidata_id"], {"id": entry["wikidata_id"], "categories": [], "discovery_sitelinks": 0})
        record["reviewed_seed"] = True
    result = {"schema": 1, "categories": CATEGORIES, "entries": sorted(rows.values(), key=lambda x: int(x["id"][1:])), "data_license": "CC0-1.0"}
    (ROOT / "discovery.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    return result


def fetch(discovery):
    folder = ROOT / "batches"
    folder.mkdir(exist_ok=True)
    ids = [entry["id"] for entry in discovery["entries"]]
    records = []
    for offset in range(0, len(ids), 50):
        batch = ids[offset:offset + 50]
        token = hashlib.sha256("|".join(batch).encode()).hexdigest()[:12]
        target = folder / f"{offset:05}-{token}.json.gz"
        if target.exists():
            data = json.loads(gzip.decompress(target.read_bytes()))
        else:
            params = {"action": "wbgetentities", "ids": "|".join(batch), "props": "info|labels|descriptions|aliases|claims|sitelinks", "languages": "en|zh|zh-hans|zh-cn", "format": "json", "maxlag": "30"}
            raw, data = request("https://www.wikidata.org/w/api.php?" + urllib.parse.urlencode(params), "application/json")
            if "entities" not in data:
                raise ValueError("Entity batch missing entities")
            target.write_bytes(gzip.compress(raw, compresslevel=9, mtime=0))
            time.sleep(0.8)
        returned = data["entities"]
        if any(qid not in returned for qid in batch):
            raise ValueError("Entity batch did not contain all requested IDs")
        for qid in batch:
            if returned[qid].get("id") != qid or not isinstance(returned[qid].get("lastrevid"), int):
                raise ValueError("Unversioned, redirected or missing entity")
        records.append({"file": target.relative_to(ROOT).as_posix(), "ids": batch, "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
        print("fetched", min(offset + 50, len(ids)), "/", len(ids), flush=True)
    (ROOT / "batches.json").write_text(json.dumps({"schema": 1, "data_license": "CC0-1.0", "batches": records}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--fetch", action="store_true", help="Explicit public API acquisition after bounded discovery")
    parser.add_argument("--output", type=Path, required=True, help="Separate acquisition/cache directory, never data/dict")
    args = parser.parse_args()
    ROOT = args.output.resolve()
    if ROOT == REPO or REPO in ROOT.parents:
        raise ValueError("Acquisition must stay outside the approved repository")
    ROOT.mkdir(parents=True, exist_ok=True)
    discovery = discover()
    if args.fetch:
        fetch(discovery)
