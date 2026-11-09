#!/usr/bin/env python3
"""Offline EAMODEN2 guards with isolated structured-source fixtures."""
import copy
import gzip
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import sys
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('modern_entities', Path(__file__).with_name('modern-entities.py'))
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def source(qid, label='Example Tool', zh='示例工具', links=8):
    return {'id': qid, 'lastrevid': 123, 'modified': '2026-11-09T00:00:00Z', 'labels': {'en': {'value': label}, 'zh': {'value': zh}}, 'descriptions': {'en': {'value': 'software'}, 'zh': {'value': '中文软件说明'}}, 'aliases': {'en': [{'value': 'Example'}]}, 'claims': {'P31': [{'mainsnak': {'snaktype': 'value', 'datavalue': {'value': {'id': 'Q7397'}}}}]}, 'sitelinks_count': links}


class CatalogGuard(unittest.TestCase):
    def setUp(self):
        t = tempfile.TemporaryDirectory(prefix='english-assist-entities-')
        self.addCleanup(t.cleanup)
        root = Path(t.name)
        for name, file in [('CATALOG', 'catalog.json'), ('SNAPSHOT', 'snapshot.gz'), ('SEED', 'seed.json'), ('RUNTIME', 'runtime.tsv')]:
            original = getattr(M, name)
            setattr(M, name, root / file)
            self.addCleanup(setattr, M, name, original)
        self.seed = {'entries': [{'text': 'Example Tool', 'aliases': ['exampletool'], 'entity_kind': '软件', 'definition_zh': '审核中文说明', 'wikidata_id': 'Q101', 'official_url': 'https://example.invalid/'}]}
        self.snapshot = {'schema': 2, 'entities': {'Q101': source('Q101'), 'Q102': source('Q102')}, 'discovery': {'entries': [{'id': 'Q101', 'categories': ['software']}, {'id': 'Q102', 'categories': ['software']}]}}
        self.save_source()
        self.save_catalog()
        p = patch.object(M.urllib.request, 'urlopen', side_effect=AssertionError('offline check attempted network'))
        p.start()
        self.addCleanup(p.stop)

    def save_source(self):
        M.SEED.write_bytes(M.json_bytes(self.seed))
        M.SNAPSHOT.write_bytes(gzip.compress(M.json_bytes(self.snapshot), mtime=0))

    def save_catalog(self):
        raw = M.SNAPSHOT.read_bytes()
        entries, rejected = M.projected_entries(self.snapshot, self.seed)
        self.data = {'schema': 2, 'runtime_format': 'EAMODEN2', 'data_license': 'CC0-1.0', 'policy': M.POLICY, 'snapshot': {'file': 'raw/modern-entities/wikidata-expanded-snapshot.json.gz', 'sha256': M.digest(raw), 'content_sha256': M.digest(gzip.decompress(raw))}, 'reviewed_seed': {'file': 'raw/modern-entities/reviewed-seed-v1.json', 'sha256': M.digest(M.SEED.read_bytes()), 'count': 1}, 'discovered_count': len(self.snapshot['entities']), 'rejected': rejected, 'entries': entries}
        self.save()

    def save(self):
        M.CATALOG.write_bytes(M.json_bytes(self.data))
        M.RUNTIME.write_bytes(M.image(self.data['entries']).encode())

    def test_offline_roundtrip_and_preserved_seed(self):
        result = M.checked()
        self.assertEqual(len(result['entries']), 2)
        entry = next(e for e in result['entries'] if e['wikidata_id'] == 'Q101')
        self.assertEqual(entry['definition_zh'], '审核中文说明')
        self.assertEqual(entry['priority'], 875)

    def test_cross_entity_alias_collision_retains_both_identities(self):
        self.snapshot['entities']['Q102']['labels']['en']['value'] = 'Example Tool'
        self.save_source()
        self.save_catalog()
        self.assertEqual(len([e for e in M.checked()['entries'] if 'exampletool' in e['aliases']]), 2)

    def test_priority_is_frozen_source_proxy_not_catalog_edit(self):
        self.data['entries'][0]['priority'] += 1000
        self.save()
        with self.assertRaises(ValueError):
            M.checked()

    def test_revision_must_match_frozen_record(self):
        self.data['entries'][0]['wikidata_revision'] += 1
        self.save()
        with self.assertRaises(ValueError):
            M.checked()

    def test_snapshot_digest_detects_changes(self):
        self.data['snapshot']['sha256'] = '0' * 64
        self.save()
        with self.assertRaises(ValueError):
            M.checked()

    def test_chinese_seed_digest_detects_changes(self):
        M.SEED.write_bytes(M.SEED.read_bytes() + b' ')
        with self.assertRaises(ValueError):
            M.checked()

    def test_no_chinese_metadata_is_rejected_not_english_pseudodefinition(self):
        self.snapshot['entities']['Q102']['labels']['zh']['value'] = 'Example'
        self.snapshot['entities']['Q102']['descriptions']['zh']['value'] = 'software'
        self.save_source()
        self.save_catalog()
        result = M.checked()
        self.assertEqual(result['rejected'], {'no-chinese-metadata': 1})
        self.assertEqual(len(result['entries']), 1)

    def test_unchanged_brand_name_can_use_real_chinese_description(self):
        self.snapshot['entities']['Q102']['labels']['zh']['value'] = 'Example'
        self.save_source()
        self.save_catalog()
        entry = next(e for e in M.checked()['entries'] if e['wikidata_id'] == 'Q102')
        self.assertEqual(entry['definition_zh'], '中文软件说明')

    def test_real_chinese_label_without_description_uses_explicit_category(self):
        del self.snapshot['entities']['Q102']['descriptions']['zh']
        self.save_source()
        self.save_catalog()
        entry = next(e for e in M.checked()['entries'] if e['wikidata_id'] == 'Q102')
        self.assertEqual(entry['definition_zh'], '示例工具（软件）')
        self.assertEqual(entry['definition_origin'], 'wikidata-zh-label-and-category')

    def test_untyped_and_human_sources_do_not_enter(self):
        self.snapshot['discovery']['entries'][1]['categories'] = []
        self.save_source()
        self.save_catalog()
        self.assertEqual(M.checked()['rejected'], {'missing-type-discovery': 1})
        self.snapshot['discovery']['entries'][1]['categories'] = ['companies']
        self.snapshot['entities']['Q102']['claims']['P31'][0]['mainsnak']['datavalue']['value']['id'] = 'Q5'
        self.save_source()
        self.save_catalog()
        self.assertEqual(M.checked()['rejected'], {'human-outside-scope': 1})

    def test_optional_official_url_is_not_fabricated(self):
        entry = next(e for e in M.checked()['entries'] if e['wikidata_id'] == 'Q102')
        self.assertEqual(entry['official_url'], '')
        self.assertIn('oldid=123', entry['revision_url'])

    def test_aliases_are_source_derived_bounded_and_canonical_kept(self):
        self.snapshot['entities']['Q102']['aliases']['en'] = [{'value': 'Alias ' + chr(65 + i // 26) + chr(65 + i % 26)} for i in range(60)]
        self.save_source()
        self.save_catalog()
        e = next(e for e in M.checked()['entries'] if e['wikidata_id'] == 'Q102')
        self.assertEqual(len(e['aliases']), 32)
        self.assertIn('exampletool', e['aliases'])

    def test_projection_is_stable_when_source_entity_order_changes(self):
        before = M.image(M.projected_entries(self.snapshot, self.seed)[0])
        self.snapshot['entities'] = dict(reversed(list(self.snapshot['entities'].items())))
        self.assertEqual(before, M.image(M.projected_entries(self.snapshot, self.seed)[0]))

    def test_runtime_stale_and_unknown_policy_fail_closed(self):
        M.RUNTIME.write_bytes(b'# stale\n')
        with self.assertRaises(ValueError):
            M.checked()
        self.save()
        self.data['policy'] = dict(M.POLICY, id='unreviewed')
        self.save()
        with self.assertRaises(ValueError):
            M.checked()

    def test_count_and_bytes_admission_bounds(self):
        with patch.object(M, 'MAX_ROWS', 1):
            with self.assertRaises(ValueError):
                M.checked()
        with patch.object(M, 'MAX_BYTES', 10):
            with self.assertRaises(ValueError):
                M.checked()

    def test_check_mode_cannot_enter_network_or_write_authoring(self):
        for extra in [['--propose-refresh', 'separate.json'], ['--write-runtime']]:
            with self.subTest(extra=extra), patch.object(sys, 'argv', ['modern-entities.py', '--check'] + extra):
                with self.assertRaises(SystemExit) as failure:
                    M.main()
                self.assertEqual(failure.exception.code, 2)

    def test_import_rejects_untrusted_batch_paths_before_read(self):
        root = M.CATALOG.parent / 'research'
        root.mkdir()
        (root / 'discovery.json').write_bytes(b'{}')
        original_read = Path.read_bytes
        for name in ['../outside.json.gz', '/outside.json.gz', 'C:/outside.json.gz', 'batches/../outside.json.gz', 'batches\\00000-aaaaaaaaaaaa.json.gz', 'batches/00000-aaaaaaaaaaaa.json.gz:stream']:
            with self.subTest(name=name):
                (root / 'batches.json').write_bytes(M.json_bytes({'batches': [{'file': name}]}))
                reads = []
                def guarded_read(path):
                    reads.append(path)
                    return original_read(path)
                with patch.object(Path, 'read_bytes', guarded_read), self.assertRaises(ValueError):
                    M.import_research(root, root.parent / 'new-proposal')
                self.assertEqual(reads, [root / 'discovery.json', root / 'batches.json'])
                self.assertFalse((root.parent / 'new-proposal').exists())

    def test_import_rejects_external_batch_symlink_before_read(self):
        root = M.CATALOG.parent / 'linked-research'
        (root / 'batches').mkdir(parents=True)
        outside = M.CATALOG.parent / 'outside.json.gz'
        outside.write_bytes(b'not to be read')
        link = root / 'batches/00000-aaaaaaaaaaaa.json.gz'
        link.symlink_to(outside)
        (root / 'discovery.json').write_bytes(b'{}')
        (root / 'batches.json').write_bytes(M.json_bytes({'batches': [{'file': 'batches/00000-aaaaaaaaaaaa.json.gz'}]}))
        reads = []
        original_read = Path.read_bytes
        def guarded_read(path):
            reads.append(path)
            return original_read(path)
        with patch.object(Path, 'read_bytes', guarded_read), self.assertRaises(ValueError):
            M.import_research(root, root.parent / 'new-proposal')
        self.assertEqual(reads, [root / 'discovery.json', root / 'batches.json'])
        self.assertFalse((root.parent / 'new-proposal').exists())


if __name__ == '__main__':
    unittest.main()
