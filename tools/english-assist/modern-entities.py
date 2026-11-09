#!/usr/bin/env python3
"""Offline modern-entity compiler. Authoring/import/refresh writes separate proposals.
No normal build fetches data or admits search results. Standard library only.
"""
import argparse
from collections import Counter
import gzip
import hashlib
import json
from pathlib import Path
import re
import stat
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
CATALOG = ROOT / 'data/dict/modern-entities.json'
RUNTIME = ROOT / 'data/dict/modern-entities.tsv'
RAW = ROOT / 'data/dict/raw/modern-entities'
SNAPSHOT = RAW / 'wikidata-expanded-snapshot.json.gz'
SEED = RAW / 'reviewed-seed-v1.json'
HEADER = '# EAMODEN2\ttext\taliases\tkind\tdefinition_zh\twikidata_id\trevision\tofficial_url\tpriority\n'
MAX_ROWS, MAX_ALIASES, MAX_BYTES = 20000, 200000, 16 * 1024 * 1024
CATEGORY = {'programming-languages': ('编程语言', 30), 'software': ('软件', 25), 'companies': ('公司', 20), 'organizations': ('组织', 15), 'products': ('产品', 10), 'websites': ('网站', 5)}
POLICY = {'id': 'wikidata-bounded-zh-v1', 'languages': ['en', 'zh-hans', 'zh-cn', 'zh'], 'admission': 'Bounded typed WDQS discoveries with English and Chinese labels; no humans; ASCII English canonical spelling; Chinese description or Chinese label plus source category. Reviewed seed overrides retained.', 'priority': 'min(frozen_sitelink_count,65535)*100 + category_bonus (programming-language30/software25/company20/organization15/product10/website5) + reviewed_seed50; source visibility proxy within entities, not corpus word frequency', 'bounds': {'rows': MAX_ROWS, 'aliases': MAX_ALIASES, 'bytes': MAX_BYTES}}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode('utf-8')


def normalized(value):
    return re.sub('[^a-z]', '', value.lower())


def safe_text(value, limit):
    return isinstance(value, str) and 0 < len(value) <= limit and not any(ord(c) < 32 or ord(c) == 127 for c in value)


def chinese(value):
    return isinstance(value, str) and bool(re.search(r'[\u3400-\u9fff]', value))


def language_value(entity, field, languages):
    for language in languages:
        value = entity.get(field, {}).get(language, {}).get('value', '')
        if safe_text(value, 1000):
            return value
    return ''


def claim_values(entity, prop):
    result = []
    for claim in entity.get('claims', {}).get(prop, []):
        if claim.get('rank') == 'deprecated':
            continue
        snak = claim.get('mainsnak', {})
        if snak.get('snaktype') == 'value':
            result.append(snak.get('datavalue', {}).get('value'))
    return result


def source_entry(entity, discovery, seed=None):
    qid, revision = entity.get('id'), entity.get('lastrevid')
    if not isinstance(qid, str) or not re.fullmatch(r'Q[1-9][0-9]*', qid) or type(revision) is not int or revision <= 0 or not entity.get('modified'):
        return None, 'missing-version'
    types = sorted({v['id'] for v in claim_values(entity, 'P31') if isinstance(v, dict) and re.fullmatch(r'Q[1-9][0-9]*', v.get('id', ''))})
    if 'Q5' in types and not seed:
        return None, 'human-outside-scope'
    categories = sorted(set(discovery.get('categories', [])))
    if any(c not in CATEGORY for c in categories) or (not categories and not seed):
        return None, 'missing-type-discovery'
    category = max(categories, key=lambda c: CATEGORY[c][1]) if categories else None
    kind, bonus = CATEGORY[category] if category else (seed['entity_kind'], 0)
    label_en = language_value(entity, 'labels', ['en'])
    description_en = language_value(entity, 'descriptions', ['en'])
    zh_label = language_value(entity, 'labels', ['zh-hans', 'zh-cn', 'zh'])
    zh_description = language_value(entity, 'descriptions', ['zh-hans', 'zh-cn', 'zh'])
    names = [label_en] + [x.get('value', '') for x in entity.get('aliases', {}).get('en', [])]
    if seed:
        text, aliases = seed['text'], seed['aliases']
        definition, kind, origin = seed['definition_zh'], seed['entity_kind'], 'reviewed-seed-v1'
    else:
        if not chinese(zh_label) and not chinese(zh_description):
            return None, 'no-chinese-metadata'
        candidates = [x for x in names if safe_text(x, 96) and x.isascii() and re.fullmatch(r'[a-z]{2,64}', normalized(x))]
        if not candidates:
            return None, 'no-ascii-english-spelling'
        text = candidates[0]
        aliases = sorted({normalized(x) for x in candidates})[:32]
        canonical = normalized(text)
        if canonical not in aliases:
            aliases = sorted(set(aliases[:-1] + [canonical]))
        if chinese(zh_description) and safe_text(zh_description, 160):
            definition = zh_description if not chinese(zh_label) or zh_label in zh_description else zh_label + '；' + zh_description
            origin = 'wikidata-zh-description'
        else:
            definition, origin = zh_label + '（' + kind + '）', 'wikidata-zh-label-and-category'
    if not safe_text(definition, 192) or not chinese(definition):
        return None, 'unbounded-chinese-annotation'
    official = seed.get('official_url', '') if seed else ''
    if not seed:
        websites = sorted({v for v in claim_values(entity, 'P856') if isinstance(v, str) and v.startswith('https://') and len(v.encode('utf-8')) <= 1024 and safe_text(v, 1024)})
        official = websites[0] if websites else ''
    count = entity.get('sitelinks_count', len(entity.get('sitelinks', {})))
    if type(count) is not int or count < 0:
        return None, 'invalid-sitelink-count'
    return {'text': text, 'aliases': aliases, 'entity_kind': kind, 'definition_zh': definition, 'definition_origin': origin, 'wikidata_id': qid, 'wikidata_revision': revision, 'wikidata_modified': entity['modified'], 'wikidata_label_en': label_en, 'wikidata_description_en': description_en, 'wikidata_label_zh': zh_label, 'wikidata_description_zh': zh_description, 'wikidata_aliases_en': [x.get('value', '') for x in entity.get('aliases', {}).get('en', [])], 'wikidata_aliases_zh': {lang: [x.get('value', '') for x in entity.get('aliases', {}).get(lang, [])] for lang in ['zh-hans', 'zh-cn', 'zh'] if entity.get('aliases', {}).get(lang)}, 'instance_of': types, 'discovery_categories': categories, 'sitelinks_count': count, 'sitelinks_source_revision': entity.get('sitelinks_source_revision', revision), 'priority': min(count, 65535) * 100 + bonus + (50 if seed else 0), 'revision_url': f'https://www.wikidata.org/w/index.php?title={qid}&oldid={revision}', 'official_url': official}, None


def image(entries):
    return HEADER + ''.join('\t'.join([e['text'], ','.join(e['aliases']), e['entity_kind'], e['definition_zh'], e['wikidata_id'], str(e['wikidata_revision']), e['official_url'], str(e['priority'])]) + '\n' for e in entries)


def projected_entries(snapshot, seed):
    reviewed = {e['wikidata_id']: e for e in seed['entries']}
    discovery = {e['id']: e for e in snapshot['discovery']['entries']}
    entries, rejected = [], Counter()
    for qid, entity in sorted(snapshot['entities'].items()):
        if entity.get('id') != qid:
            raise ValueError('Source dictionary key and actual entity identity differ')
        result, reason = source_entry(entity, discovery.get(qid, {}), reviewed.get(qid))
        if result:
            entries.append(result)
        else:
            rejected[reason] += 1
    if not set(reviewed).issubset({e['wikidata_id'] for e in entries}):
        raise ValueError('A reviewed seed identity was lost during projection')
    entries.sort(key=lambda e: (e['aliases'][0], -e['priority'], e['wikidata_id']))
    return entries, dict(sorted(rejected.items()))


def checked(check_runtime=True):
    data = json.loads(CATALOG.read_text(encoding='utf-8'))
    raw = SNAPSHOT.read_bytes()
    content = gzip.decompress(raw)
    snapshot = json.loads(content)
    seed_raw = SEED.read_bytes()
    seed = json.loads(seed_raw)
    if data.get('schema') != 2 or data.get('runtime_format') != 'EAMODEN2' or data.get('data_license') != 'CC0-1.0' or data.get('policy') != POLICY:
        raise ValueError('Unsupported approved entity schema/policy')
    if data.get('snapshot') != {'file': 'raw/modern-entities/wikidata-expanded-snapshot.json.gz', 'sha256': digest(raw), 'content_sha256': digest(content)}:
        raise ValueError('Frozen Wikidata source digest mismatch')
    if data.get('reviewed_seed') != {'file': 'raw/modern-entities/reviewed-seed-v1.json', 'sha256': digest(seed_raw), 'count': len(seed['entries'])}:
        raise ValueError('Reviewed Chinese seed source digest mismatch')
    entries, rejected = projected_entries(snapshot, seed)
    if data.get('entries') != entries or data.get('rejected') != rejected or data.get('discovered_count') != len(snapshot['entities']):
        raise ValueError('Approved entries do not reproduce frozen sources/filter policy')
    if not 1 <= len(entries) <= MAX_ROWS or sum(len(e['aliases']) for e in entries) > MAX_ALIASES:
        raise ValueError('Entity count or alias budget exceeded')
    ids = set()
    for e in entries:
        if e['wikidata_id'] in ids:
            raise ValueError('Duplicate entity identity')
        ids.add(e['wikidata_id'])
        keys = e['aliases']
        if not keys or len(keys) > 32 or keys != sorted(set(keys)) or any(not re.fullmatch(r'[a-z]{2,64}', key) for key in keys):
            raise ValueError('Unbounded or malformed source alias')
        if not safe_text(e['text'], 96) or not e['text'].isascii() or normalized(e['text']) not in keys or not safe_text(e['entity_kind'], 12):
            raise ValueError('Invalid canonical spelling or kind')
        url = e['official_url']
        if url and (not url.startswith('https://') or not safe_text(url, 1024) or len(url.encode()) > 1024):
            raise ValueError('Invalid optional official URL')
    expected = image(entries).encode('utf-8')
    if len(expected) > MAX_BYTES:
        raise ValueError('Embedded entity TSV exceeds runtime byte budget')
    if check_runtime and RUNTIME.read_bytes() != expected:
        raise ValueError('Runtime TSV differs from approved catalog; regenerate explicitly after review')
    return data


def research_batch_path(folder, relative):
    # The collector emits this single fixed subdirectory plus a strict basename.
    # Do not accept arbitrary relative paths, Windows drive/NT syntax or streams.
    if not isinstance(relative, str) or not re.fullmatch(r'batches/[0-9]{5}-[a-f0-9]{12}\.json\.gz', relative):
        raise ValueError('Research batch filename is outside the collector contract')
    root = folder.resolve(strict=True)
    path = root / 'batches' / relative.removeprefix('batches/')
    for component in (folder, root / 'batches', path):
        metadata = component.lstat()
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            raise ValueError('Research batch path contains a symlink or reparse point')
    if not path.resolve(strict=True).is_relative_to(root) or not path.is_file():
        raise ValueError('Research batch path escaped its selected directory')
    return path


def import_research(folder, output):
    resolved_output = output.resolve()
    if resolved_output == ROOT or ROOT in resolved_output.parents or output.exists():
        raise ValueError('An import proposal must be a new directory outside the approved repository')
    discovery = json.loads((folder / 'discovery.json').read_bytes())
    batches = json.loads((folder / 'batches.json').read_bytes())['batches']
    entities, sources = {}, []
    for batch in batches:
        raw = research_batch_path(folder, batch['file']).read_bytes()
        if digest(raw) != batch['sha256']:
            raise ValueError('Research batch digest changed')
        response = json.loads(gzip.decompress(raw))
        for qid in batch['ids']:
            source = response['entities'][qid]
            entities[qid] = {k: source[k] for k in ('id', 'lastrevid', 'modified', 'labels', 'descriptions', 'aliases') if k in source}
            entities[qid]['claims'] = {p: source.get('claims', {}).get(p, []) for p in ('P31', 'P856')}
            entities[qid]['sitelinks_count'] = len(source.get('sitelinks', {}))
        sources.append({'ids': batch['ids'], 'compressed_response_sha256': digest(raw), 'response_sha256': digest(gzip.decompress(raw)), 'api': 'https://www.wikidata.org/w/api.php?action=wbgetentities&props=info|labels|descriptions|aliases|claims|sitelinks&languages=en|zh|zh-hans|zh-cn'})
    seed_raw = SEED.read_bytes()
    seed = json.loads(seed_raw)
    old = json.loads(gzip.decompress((RAW / 'wikidata-snapshot.json.gz').read_bytes()))
    for e in seed['entries']:
        qid = e['wikidata_id']
        count = entities.get(qid, {}).get('sitelinks_count', 0)
        visibility_revision = entities.get(qid, {}).get('lastrevid', e['wikidata_revision'])
        entities[qid] = old['entities'][qid]
        entities[qid]['sitelinks_count'] = count
        entities[qid]['sitelinks_source_revision'] = visibility_revision
    queries = []
    for path in sorted(folder.glob('discover-*.sparql')):
        response = path.with_suffix('.json')
        if not response.exists():
            continue  # Timed-out queries did not supply source data.
        queries.append({'category': path.stem.removeprefix('discover-'), 'query': path.read_text(encoding='utf-8'), 'query_sha256': digest(path.read_bytes()), 'result_sha256': digest(response.read_bytes())})
    snapshot = {'schema': 2, 'data_license': 'CC0-1.0', 'projection_note': 'Selected-field projection of bounded public API batches, not a full dump or original HTTP body; prior reviewed seed revisions retained.', 'discovery': discovery, 'queries': queries, 'batches': sources, 'entities': entities}
    source_raw = json_bytes(snapshot)
    source_gzip = gzip.compress(source_raw, compresslevel=9, mtime=0)
    entries, rejected = projected_entries(snapshot, seed)
    data = {'schema': 2, 'runtime_format': 'EAMODEN2', 'scope': 'Bounded typed Wikidata entities with real Chinese metadata plus a preserved reviewed seed; not complete, not dictionary definitions or a currentness guarantee.', 'data_license': 'CC0-1.0', 'policy': POLICY, 'snapshot': {'file': 'raw/modern-entities/wikidata-expanded-snapshot.json.gz', 'sha256': digest(source_gzip), 'content_sha256': digest(source_raw)}, 'reviewed_seed': {'file': 'raw/modern-entities/reviewed-seed-v1.json', 'sha256': digest(seed_raw), 'count': len(seed['entries'])}, 'discovered_count': len(entities), 'rejected': rejected, 'entries': entries}
    output.mkdir(parents=True, exist_ok=False)
    (output / 'wikidata-expanded-snapshot.json.gz').write_bytes(source_gzip)
    (output / 'modern-entities.json').write_bytes(json_bytes(data))
    (output / 'modern-entities.tsv').write_bytes(image(entries).encode('utf-8'))
    print(f'Separate import proposal: {len(entries)} admitted / {len(entities)} source IDs; rejected={rejected}')


def proposal(data, output):
    entities = {}
    ids = [e['wikidata_id'] for e in data['entries']]
    for start in range(0, len(ids), 50):
        query = urllib.parse.urlencode({'action': 'wbgetentities', 'ids': '|'.join(ids[start:start + 50]), 'languages': 'en|zh|zh-hans|zh-cn', 'props': 'info|labels|descriptions|aliases|claims|sitelinks', 'format': 'json', 'maxlag': '30'})
        request = urllib.request.Request('https://www.wikidata.org/w/api.php?' + query, headers={'User-Agent': 'EnglishAssistDictionaryResearch/0.6 (explicit separate review proposal)'})
        with urllib.request.urlopen(request, timeout=45) as response:
            fetched = json.load(response)
        if 'error' in fetched:
            raise ValueError('Wikidata rejected refresh; approved inputs unchanged')
        entities.update(fetched['entities'])
    with output.open('xb') as stream:
        stream.write(json_bytes({'schema': 2, 'review_required': True, 'note': 'Separate proposal: review Chinese sources, identity, aliases, type/filter policy, revisions and priority before admitting.', 'entities': entities}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Offline validation (default)')
    parser.add_argument('--write-runtime', action='store_true', help='Explicit offline TSV regeneration after source review')
    parser.add_argument('--propose-refresh', type=Path, help='Network refresh to a separate new review JSON')
    parser.add_argument('--import-research', type=Path, help='Saved discovery/batch directory; no network')
    parser.add_argument('--output-proposal', type=Path, help='New directory for source/catalog/runtime review proposal')
    args = parser.parse_args()
    if args.check and (args.write_runtime or args.propose_refresh or args.import_research or args.output_proposal):
        parser.error('--check is strictly offline/read-only and cannot be combined with authoring modes')
    if args.output_proposal and not args.import_research:
        parser.error('--output-proposal requires --import-research')
    if args.import_research:
        if not args.output_proposal or args.check or args.write_runtime or args.propose_refresh:
            parser.error('--import-research requires only --output-proposal')
        import_research(args.import_research, args.output_proposal)
        return
    if args.write_runtime:
        data = checked(check_runtime=False)
        RUNTIME.write_bytes(image(data['entries']).encode('utf-8'))
    data = checked()
    if args.propose_refresh:
        if args.propose_refresh.resolve() in (CATALOG.resolve(), RUNTIME.resolve(), SNAPSHOT.resolve(), SEED.resolve()):
            raise ValueError('A proposal cannot overwrite approved inputs')
        proposal(data, args.propose_refresh)
    print(f"Approved modern entities: {len(data['entries'])} IDs; {data['discovered_count']} source IDs; rejected={data['rejected']}; offline sources/runtime consistent")


if __name__ == '__main__':
    main()
