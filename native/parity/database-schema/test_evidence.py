#!/usr/bin/env python3
"""Synthetic negative gates; all write fixtures under private lane build only."""
import copy
import importlib.util
import io
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

LANE = Path(__file__).resolve().parent
ROOT = LANE.parents[2]
BUILD = ROOT / 'native/build/database-schema'


def module(name):
    """Import explicit source script without dependency installation."""
    spec = importlib.util.spec_from_file_location(name, LANE / (name + '.py'))
    assert spec is not None and spec.loader is not None
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


export = module('export')
coverage = module('coverage')
artifacts = module('check_artifacts')


def invoke(function, args, root):
    """Capture diagnostics; tests never print malicious payloads or paths."""
    stdout, stderr = io.StringIO(), io.StringIO()
    with redirect_stdout(stdout), redirect_stderr(stderr):
        code = function(args, root)
    return code, stdout.getvalue(), stderr.getvalue()


class SchemaEvidence(unittest.TestCase):
    """Actual 23-schema positive check plus corruption/drift/semantic negatives."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=BUILD, prefix='negative-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        files = export.source_paths() + [
            'native/src/database_schema_data.rs',
            str(export.LANE / 'source-hashes.json'), str(export.LANE / 'export.py'),
            str(export.LANE / 'fixtures.json'), str(export.LANE / 'artifact-hashes.json'),
            str(export.BUILD / 'rust-ddl.json'),
        ]
        for file in files:
            target = self.root / file
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / file, target)
        self.current_path = self.root / 'core/data/schemas/com.ehviewer.core.database.EhDatabase/24.json'

    def failure(self):
        self.assertEqual(invoke(export.main, ['--check'], self.root), (1, '', export.FAILURE))

    def mutate_replay(self, transform):
        path = self.root / export.BUILD / 'rust-ddl.json'
        emitted = export.load(path.read_bytes())
        value = next(v for v in emitted if v['name'] == 'EhDatabase' and v['version'] == 24)
        transform(value)
        path.write_bytes(export.encoded(emitted))
        self.failure()

    def test_loader_and_artifact_parser_failures_are_payload_free(self):
        manifest = self.root / export.LANE / 'artifact-sha256.json'
        for raw in [b'{"private-marker":', b'\xff', b'[' * 2000]:
            with self.assertRaises(ValueError) as caught:
                export.load(raw)
            self.assertEqual(str(caught.exception), 'invalid evidence')
            self.assertTrue(caught.exception.__suppress_context__)
            manifest.write_bytes(raw)
            with self.assertRaises(ValueError) as caught:
                artifacts.check(self.root)
            self.assertEqual(str(caught.exception), 'invalid inventory')
            self.assertTrue(caught.exception.__suppress_context__)
        manifest.unlink()
        with self.assertRaises(ValueError) as caught:
            artifacts.check(self.root)
        self.assertEqual(str(caught.exception), 'invalid inventory')
        self.assertEqual(set(export.VERSION_PRAGMAS), {v for versions in export.VERSIONS.values() for v in versions})
        for version, statement in export.VERSION_PRAGMAS.items():
            self.assertEqual(statement, f'PRAGMA user_version={version}')

    def test_actual_all23_positive(self):
        code, output, error = invoke(export.main, ['--check'], self.root)
        self.assertEqual((code, error), (0, ''))
        self.assertIn('23-version SQLite differential passed', output)

    def test_empty_recovery_hashes_invalid(self):
        (self.root / export.LANE / 'source-hashes.json').write_bytes(b'')
        self.failure()

    def test_source_byte_corruption(self):
        self.current_path.write_bytes(self.current_path.read_bytes() + b' ')
        self.failure()

    def test_malformed_source_with_rebound_hash_still_rejected(self):
        self.current_path.write_bytes(b'{"private-payload":')
        inventory = self.root / export.LANE / 'source-hashes.json'
        hashes = export.load(inventory.read_bytes())
        hashes[str(self.current_path.relative_to(self.root))] = export.digest(self.current_path.read_bytes())
        inventory.write_bytes(export.encoded(hashes))
        self.failure()

    def test_schema_shape_with_rebound_hash_still_rejected(self):
        raw = export.load(self.current_path.read_bytes())
        raw['database']['entities'][0]['fields'][0]['notNull'] = 'private-payload'
        self.current_path.write_bytes(export.encoded(raw))
        inventory = self.root / export.LANE / 'source-hashes.json'
        hashes = export.load(inventory.read_bytes())
        hashes[str(self.current_path.relative_to(self.root))] = export.digest(self.current_path.read_bytes())
        inventory.write_bytes(export.encoded(hashes))
        self.failure()

    def test_missing_source_version(self):
        self.current_path.unlink()
        self.failure()

    def test_extra_source_version_no_synthetic_history(self):
        (self.current_path.parent / '1.json').write_bytes(b'{}')
        self.failure()

    def test_version_number_cannot_substitute_shared_identity(self):
        raw = export.load(self.current_path.read_bytes())
        raw['database']['version'] = 23
        with self.assertRaises(ValueError):
            export.validate_database(raw['database'], 'EhDatabase', 24)

    def test_original_long_preserved(self):
        first = next(v for v in export.sources(self.root) if v['name'] == 'SearchDatabase' and v['version'] == 1)
        date = next(f for f in first['database']['entities'][0]['fields'] if f['columnName'] == 'date')
        self.assertEqual(date['affinity'], 'LONG')
        self.assertEqual(export.replay(first)['tables']['suggestions']['table_info'][1][2], 'LONG')

    def test_wrong_sql_nullability_fails_real_pragma_not_hash(self):
        def mutate(value):
            value['ddl'][0] = value['ddl'][0].replace('`TOKEN` TEXT NOT NULL', '`TOKEN` TEXT')
        self.mutate_replay(mutate)

    def test_identity_setup_mutation_fails_sqlite_not_hash(self):
        def mutate(value):
            value['ddl'][-1] = value['ddl'][-1].replace(value['identity'], 'f' * 32)
        self.mutate_replay(mutate)

    def test_trigger_insert_removed_fails_real_fts_match(self):
        def mutate(value):
            value['ddl'] = [q for q in value['ddl'] if 'room_fts_content_sync_GALLERIES_FTS_AFTER_INSERT' not in q]
        self.mutate_replay(mutate)

    def test_trigger_update_removed_fails_real_fts_match(self):
        def mutate(value):
            value['ddl'] = [q for q in value['ddl'] if 'room_fts_content_sync_GALLERIES_FTS_AFTER_UPDATE' not in q]
        self.mutate_replay(mutate)

    def test_trigger_delete_removed_fails_real_fts_match(self):
        def mutate(value):
            value['ddl'] = [q for q in value['ddl'] if 'room_fts_content_sync_GALLERIES_FTS_BEFORE_DELETE' not in q]
        self.mutate_replay(mutate)

    def test_fk_action_changed_fails_real_pragma(self):
        def mutate(value):
            value['ddl'] = [q.replace('ON UPDATE CASCADE ON DELETE SET NULL', 'ON UPDATE NO ACTION ON DELETE NO ACTION') for q in value['ddl']]
        self.mutate_replay(mutate)

    def test_wrong_index_column_fails_real_index_xinfo(self):
        def mutate(value):
            value['ddl'] = [q.replace('ON `DOWNLOADS` (`LABEL`)', 'ON `DOWNLOADS` (`TIME`)') for q in value['ddl']]
        self.mutate_replay(mutate)

    def test_malformed_sql_fails_real_execution(self):
        self.mutate_replay(lambda value: value['ddl'].append('private-payload is not SQL'))

    def test_descriptor_drift(self):
        self.mutate_replay(lambda value: value['database']['entities'][0]['fields'][0].update(notNull=False))

    def test_compiled_replay_missing_version(self):
        path = self.root / export.BUILD / 'rust-ddl.json'
        emitted = export.load(path.read_bytes())
        path.write_bytes(export.encoded(emitted[:-1]))
        self.failure()

    def test_compiled_replay_extra_version(self):
        path = self.root / export.BUILD / 'rust-ddl.json'
        emitted = export.load(path.read_bytes())
        emitted.append(copy.deepcopy(emitted[-1]))
        path.write_bytes(export.encoded(emitted))
        self.failure()

    def test_generated_data_drift_no_refresh(self):
        path = self.root / 'native/src/database_schema_data.rs'
        path.write_bytes(path.read_bytes() + b'// drift\n')
        before = path.read_bytes()
        self.failure()
        self.assertEqual(path.read_bytes(), before)

    def test_frozen_fixture_drift_no_refresh(self):
        path = self.root / export.LANE / 'fixtures.json'
        path.write_bytes(path.read_bytes() + b' ')
        before = path.read_bytes()
        self.failure()
        self.assertEqual(path.read_bytes(), before)

    def test_exporter_binding_drift(self):
        path = self.root / export.LANE / 'export.py'
        path.write_bytes(path.read_bytes() + b'\n# drift\n')
        self.failure()

    def test_corrupt_manifest(self):
        (self.root / export.LANE / 'artifact-hashes.json').write_bytes(b'private-payload')
        self.failure()

    def test_truncated_compiled_replay(self):
        (self.root / export.BUILD / 'rust-ddl.json').write_bytes(b'[{"private-payload":')
        self.failure()

    def test_source_read_failure_generic(self):
        with patch.object(Path, 'read_bytes', side_effect=PermissionError('private-payload/private-path')):
            self.failure()

    def test_recursion_failure_generic(self):
        with patch.object(export, 'check', side_effect=RecursionError('private-payload')):
            self.failure()

    def test_json_duplicates_nan_utf8_rejected(self):
        for raw in (b'{"k":1,"k":2}', b'{"k":NaN}', b'\xff'):
            with self.subTest(raw_length=len(raw)), self.assertRaises(ValueError):
                export.load(raw)

    def test_args_never_echo_input(self):
        self.assertEqual(invoke(export.main, ['--private-path'], self.root), (1, '', export.FAILURE))

    def test_explicit_generation_candidates_only_never_overwrite(self):
        frozen = {p: (self.root / p).read_bytes() for p in (
            'native/src/database_schema_data.rs', str(export.LANE / 'fixtures.json'), str(export.LANE / 'artifact-hashes.json'))}
        code, _, error = invoke(export.main, ['--generate'], self.root)
        self.assertEqual((code, error), (0, ''))
        self.assertEqual(invoke(export.main, ['--generate'], self.root), (1, '', export.FAILURE))
        for path, before in frozen.items():
            self.assertEqual((self.root / path).read_bytes(), before)


class CoverageEvidence(unittest.TestCase):
    """Synthetic checker tests; never substitute for real measured coverage."""

    @staticmethod
    def report(count=100, covered=95):
        return {'data': [{'files': [{'filename': coverage.OWNER, 'summary': {
            name: {'count': count, 'covered': covered} for name in ('lines', 'functions')}}]}]}

    def test_threshold_and_owner_only(self):
        report = self.report()
        report['data'][0]['files'].append({'filename': 'native/src/preferences.rs'})
        self.assertEqual(coverage.check(report), {'lines': 95.0, 'functions': 95.0})

    def test_bad_empty_boolean_subthreshold_counts(self):
        for count, covered in [(0, 0), (True, True), (100, 94), (100, 101), (100, -1), (100.0, 100), ('100', 100)]:
            with self.subTest(count_type=type(count).__name__), self.assertRaises(ValueError):
                coverage.check(self.report(count, covered))

    def test_dotdot_owner_is_same_file_not_omission_or_duplicate(self):
        report = self.report()
        entry = report['data'][0]['files'][0]
        entry['filename'] = '/private-root/native/tests/../src/database_schema.rs'
        self.assertEqual(coverage.check(report), {'lines': 95.0, 'functions': 95.0})
        report['data'][0]['files'].append(self.report()['data'][0]['files'][0])
        with self.assertRaises(ValueError):
            coverage.check(report)

    def test_missing_or_duplicate_owner(self):
        for files in [[], self.report()['data'][0]['files'] * 2]:
            with self.assertRaises(ValueError):
                coverage.check({'data': [{'files': files}]})

    def test_read_shape_json_encoding_failures_are_generic(self):
        with tempfile.TemporaryDirectory(dir=BUILD, prefix='coverage-negative-') as directory:
            root = Path(directory)
            self.assertEqual(invoke(coverage.main, [], root), (1, '', coverage.FAILURE))
            path = root / coverage.REPORT
            path.parent.mkdir(parents=True)
            for raw in [b'', b'private-payload', b'\xff', b'{}', b'{"data":null}', b'{"data":[{"files":[{"filename":null}]}]}']:
                path.write_bytes(raw)
                self.assertEqual(invoke(coverage.main, [], root), (1, '', coverage.FAILURE))
            with patch.object(Path, 'read_bytes', side_effect=PermissionError('private-path')):
                self.assertEqual(invoke(coverage.main, [], root), (1, '', coverage.FAILURE))
            self.assertEqual(invoke(coverage.main, ['private-path'], root), (1, '', coverage.FAILURE))


class ArtifactEvidence(unittest.TestCase):
    """Source/fixture checksum inventory failures stay redacted and fail closed."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=BUILD, prefix='artifact-negative-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for file in (*artifacts.ARTIFACTS, str(artifacts.LANE / 'artifact-sha256.json')):
            target = self.root / file
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / file, target)

    def failure(self):
        self.assertEqual(invoke(artifacts.main, [], self.root), (1, '', artifacts.FAILURE))

    def test_actual_inventory(self):
        self.assertEqual(invoke(artifacts.main, [], self.root)[0], 0)

    def test_malformed_empty_read_and_payload_args(self):
        path = self.root / artifacts.LANE / 'artifact-sha256.json'
        for raw in [b'', b'private-payload', b'\xff', b'{}', b'[]']:
            path.write_bytes(raw)
            self.failure()
        with patch.object(Path, 'read_bytes', side_effect=PermissionError('private-path')):
            self.failure()
        self.assertEqual(invoke(artifacts.main, ['private-path'], self.root), (1, '', artifacts.FAILURE))

    def test_source_drift_and_missing_fixture(self):
        (self.root / 'native/src/database_schema.rs').write_bytes(b'private-payload')
        self.failure()
        (self.root / artifacts.LANE / 'fixtures.json').unlink()
        self.failure()

    def test_extra_and_missing_inventory_paths(self):
        path = self.root / artifacts.LANE / 'artifact-sha256.json'
        original = export.load(path.read_bytes())
        extra = dict(original, extra='private-payload')
        path.write_bytes(export.encoded(extra))
        self.failure()
        del original['native/src/database_schema.rs']
        path.write_bytes(export.encoded(original))
        self.failure()


if __name__ == '__main__':
    unittest.main()
