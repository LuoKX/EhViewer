#!/usr/bin/env python3
"""Source-bound schema exporter and real host SQLite differential replay.

--generate writes candidates ONLY under native/build/database-schema/candidate.
--check never writes or refreshes frozen source/data/fixture evidence.
All input/read/SQLite failures have one payload-free diagnostic.
"""
import hashlib
import json
import re
import sqlite3
import subprocess
import sys
from pathlib import Path
from typing import Any, cast

ROOT = Path(__file__).resolve().parents[3]
LANE = Path('native/parity/database-schema')
BUILD = Path('native/build/database-schema')
VERSIONS = {'EhDatabase': tuple(range(4, 25)), 'SearchDatabase': (1, 2)}
# Execute only fixed statements from retained version constants, not caller text.
VERSION_PRAGMAS = {v: f'PRAGMA user_version={v}' for versions in VERSIONS.values() for v in versions}
FAILURE = 'Schema evidence failed\n'


def require(condition: bool) -> None:
    """Reject any incomplete/changed contract without including input."""
    if not condition:
        raise ValueError('invalid evidence')


def digest(raw):
    """Hash exact bytes, not reserialized source JSON."""
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    """Canonical UTF8 JSON with a final newline."""
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode()


def unique_object(pairs):
    """Reject duplicate JSON keys rather than silently selecting last value."""
    result = {}
    for key, value in pairs:
        require(key not in result)
        result[key] = value
    return result


def load(raw: bytes) -> Any:
    """Strict JSON; malformed encodings, duplicate keys and NaN reject."""
    try:
        return json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object,
                          parse_constant=lambda _: require(False))
    except (ValueError, RecursionError):
        raise ValueError('invalid evidence') from None


def source_paths():
    """Fixed original inventories; never discover application data."""
    return [f'core/data/schemas/com.ehviewer.core.database.{name}/{v}.json'
            for name, versions in VERSIONS.items() for v in versions]


def validate_database(database: dict[str, Any], name: str, version: int) -> None:
    """Validate source shape as well as exact bound source bytes."""
    require(set(database) <= {'version', 'identityHash', 'entities', 'setupQueries', 'views'})
    require(type(database['version']) is int and database['version'] == version)
    require(re.fullmatch('[0-9a-f]{32}', database['identityHash']) is not None)
    require(not database.get('views', []))
    require(type(database['setupQueries']) is list and len(database['setupQueries']) == 2)
    require(all(type(q) is str for q in database['setupQueries']))
    require(type(database['entities']) is list and len(database['entities']) > 0)
    entities = cast(list[dict[str, Any]], database['entities'])
    require(len({e['tableName'] for e in entities}) == len(entities))
    for entity in entities:
        require(set(entity) <= {'tableName', 'createSql', 'fields', 'primaryKey', 'indices',
                                'foreignKeys', 'ftsVersion', 'ftsOptions', 'contentSyncTriggers'})
        require(re.fullmatch('[A-Za-z_][A-Za-z_0-9]*', entity['tableName']) is not None)
        require(type(entity['createSql']) is str and '${TABLE_NAME}' in entity['createSql'])
        require(type(entity['fields']) is list and len(entity['fields']) > 0)
        fields = cast(list[dict[str, Any]], entity['fields'])
        require(len({f['columnName'] for f in fields}) == len(fields))
        for field in fields:
            require(set(field) <= {'fieldPath', 'columnName', 'affinity', 'notNull', 'defaultValue'})
            require(all(type(field[k]) is str for k in ('fieldPath', 'columnName', 'affinity')))
            # SearchDatabase 1 originally declares date as LONG; preserve it.
            require(field['affinity'] in ('INTEGER', 'TEXT', 'REAL', 'BLOB', 'NUMERIC', 'LONG'))
            require(type(field.get('notNull', False)) is bool)
            require('defaultValue' not in field or type(field['defaultValue']) is str)
        pk = entity['primaryKey']
        require(set(pk) == {'autoGenerate', 'columnNames'} and type(pk['autoGenerate']) is bool)
        columns = {f['columnName'] for f in fields}
        require(type(pk['columnNames']) is list and set(pk['columnNames']) <= columns)
        for index in entity.get('indices', []):
            require(set(index) == {'name', 'unique', 'columnNames', 'orders', 'createSql'})
            require(type(index['name']) is str and type(index['createSql']) is str)
            require(type(index['unique']) is bool and type(index['columnNames']) is list)
            require(set(index['columnNames']) <= columns)
            require(type(index['orders']) is list and
                    (not index['orders'] or len(index['orders']) == len(index['columnNames'])))
            require(all(o in ('ASC', 'DESC') for o in index['orders']))
        for fk in entity.get('foreignKeys', []):
            require(set(fk) == {'table', 'onDelete', 'onUpdate', 'columns', 'referencedColumns'})
            require(type(fk['columns']) is list and len(fk['columns']) > 0)
            require(set(fk['columns']) <= columns)
            require(type(fk['referencedColumns']) is list and
                    len(fk['referencedColumns']) == len(fk['columns']))
            require(all(type(fk[k]) is str for k in ('table', 'onDelete', 'onUpdate')))
        triggers = entity.get('contentSyncTriggers', [])
        require(type(triggers) is list and all(type(t) is str for t in triggers))
        if 'ftsVersion' in entity:
            require(entity['ftsVersion'] == 'FTS4' and type(entity['ftsOptions']) is dict)
            require(len(triggers) == 4 and 'contentSyncTriggers' not in entity['ftsOptions'])
        else:
            require(not triggers and 'ftsOptions' not in entity)
    require(name in VERSIONS)


def sources(root: Path = ROOT) -> list[dict[str, Any]]:
    """Read exactly 23 retained schemas; missing/extra versions fail closed."""
    hashes = load((root / LANE / 'source-hashes.json').read_bytes())
    require(set(hashes) == set(source_paths()))
    result = []
    for name, versions in VERSIONS.items():
        directory = root / f'core/data/schemas/com.ehviewer.core.database.{name}'
        require({p.name for p in directory.iterdir()} == {f'{v}.json' for v in versions})
        for version in versions:
            path = f'core/data/schemas/com.ehviewer.core.database.{name}/{version}.json'
            raw = (root / path).read_bytes()
            require(digest(raw) == hashes[path])
            original = load(raw)
            require(set(original) == {'formatVersion', 'database'} and original['formatVersion'] == 1)
            database = original['database']
            validate_database(database, name, version)
            result.append({'name': name, 'version': version, 'identity': database['identityHash'],
                           'source_sha256': hashes[path], 'database': database})
    return result


def ddl(database: dict[str, Any]) -> list[str]:
    """Only TABLE_NAME substitution; entity-level triggers, no invented SQL."""
    entities = database['entities']
    statements = [e['createSql'].replace('${TABLE_NAME}', e['tableName']) for e in entities]
    statements += [i['createSql'].replace('${TABLE_NAME}', e['tableName'])
                   for e in entities for i in e.get('indices', [])]
    statements += [t.replace('${TABLE_NAME}', e['tableName'])
                   for e in entities for t in e.get('contentSyncTriggers', [])]
    return statements + database['setupQueries']


def rust_string(value):
    """Source strings use raw literals so original SQL whitespace survives."""
    hashes = ''
    while '"' + hashes in value:
        hashes += '#'
    return 'r' + hashes + '"' + value + '"' + hashes


def strings(values):
    return '&[' + ','.join(rust_string(v) for v in values) + ']'


def option(value):
    return 'None' if value is None else 'Some(' + rust_string(value) + ')'


def boolean(value):
    return 'true' if value else 'false'


def rust_struct(name: str, fields: dict[str, str]) -> str:
    """Emit named Rust fields; only generated source values enter here."""
    return name + ' { ' + ','.join(f'{key}: {value}' for key, value in fields.items()) + ' }'


def entity_rust(entity: dict[str, Any]) -> str:
    """Generate full typed descriptor from original declarations."""
    columns = ','.join(rust_struct('Column', {
        'field_path': rust_string(f['fieldPath']), 'name': rust_string(f['columnName']),
        'affinity': rust_string(f['affinity']), 'not_null': boolean(f.get('notNull', False)),
        'default_value': option(f.get('defaultValue'))}) for f in entity['fields'])
    pk = entity['primaryKey']
    foreign = ','.join(rust_struct('ForeignKey', {
        'table': rust_string(f['table']), 'on_delete': rust_string(f['onDelete']),
        'on_update': rust_string(f['onUpdate']), 'columns': strings(f['columns']),
        'referenced_columns': strings(f['referencedColumns'])}) for f in entity.get('foreignKeys', []))
    indices = ','.join(rust_struct('Index', {
        'name': rust_string(i['name']), 'unique': boolean(i['unique']),
        'columns': strings(i['columnNames']), 'orders': strings(i['orders']),
        'create_sql': rust_string(i['createSql'])}) for i in entity.get('indices', []))
    fts_options = (json.dumps(entity['ftsOptions'], ensure_ascii=False, sort_keys=True, separators=(',', ':'))
                   if 'ftsOptions' in entity else None)
    return rust_struct('Entity', {
        'table': rust_string(entity['tableName']), 'create_sql': rust_string(entity['createSql']),
        'columns': '&[' + columns + ']', 'primary_key': rust_struct('PrimaryKey', {
            'auto_generate': boolean(pk['autoGenerate']), 'columns': strings(pk['columnNames'])}),
        'foreign_keys': '&[' + foreign + ']', 'indices': '&[' + indices + ']',
        'fts_version': option(entity.get('ftsVersion')), 'fts_options_json': option(fts_options),
        'content_sync_triggers': strings(entity.get('contentSyncTriggers', []))})


def rust_data(retained: list[dict[str, Any]]) -> bytes:
    """Deterministic generated Rust, formatted by already installed rustfmt."""
    text = '//! Generated retained Room contracts. See parity/database-schema/export.py.\n'
    text += 'use super::{Column, Database, Entity, ForeignKey, Index, PrimaryKey, Schema};\n'
    for name, symbol, namespace in [('EhDatabase', 'EH_SCHEMAS', 'Eh'), ('SearchDatabase', 'SEARCH_SCHEMAS', 'Search')]:
        text += f'pub(super) static {symbol}: &[Schema] = &[\n'
        for source in retained:
            if source['name'] != name:
                continue
            database = source['database']
            text += rust_struct('Schema', {
                'database': f'Database::{namespace}', 'version': str(source['version']),
                'identity_hash': rust_string(source['identity']), 'source_sha256': rust_string(source['source_sha256']),
                'entities': '&[' + ','.join(entity_rust(e) for e in database['entities']) + ']',
                'setup_queries': strings(database['setupQueries']), 'ddl': strings(ddl(database))}) + ',\n'
        text += '];\n'
    output = subprocess.run(['rustfmt', '--edition', '2024', '--emit', 'stdout'],
                            input=text.encode(), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True)
    return output.stdout


def identifier(name):
    """Quote fixed schema identifiers only; caller SQL is never accepted."""
    return '"' + name.replace('"', '""') + '"'


def rows(connection, query):
    return [list(row) for row in connection.execute(query)]


def master_sql(statement: str) -> str:
    """SQLite stores original SQL except its leading IF NOT EXISTS modifier."""
    return re.sub(r'^(CREATE(?: UNIQUE| VIRTUAL)? (?:TABLE|INDEX|TRIGGER)) IF NOT EXISTS ',
                  r'\1 ', statement)


def verify_intent(connection: sqlite3.Connection, database: dict[str, Any]) -> None:
    """Compare real PRAGMA fields/keys/indices and declared master SQL to source."""
    declared = {('table', 'room_master_table'): master_sql(database['setupQueries'][0])}
    for entity in database['entities']:
        declared[('table', entity['tableName'])] = master_sql(
            entity['createSql'].replace('${TABLE_NAME}', entity['tableName']))
        for index in entity.get('indices', []):
            declared[('index', index['name'])] = master_sql(
                index['createSql'].replace('${TABLE_NAME}', entity['tableName']))
        for trigger in entity.get('contentSyncTriggers', []):
            found = re.match(r'^CREATE TRIGGER IF NOT EXISTS ([A-Za-z_][A-Za-z_0-9]*) ', trigger)
            require(found is not None)
            if found is not None:
                declared[('trigger', found[1])] = master_sql(trigger.replace('${TABLE_NAME}', entity['tableName']))
    actual_master = {(kind, name): sql for kind, name, sql in connection.execute(
        'SELECT type,name,sql FROM sqlite_master')}
    require(all(actual_master.get(key) == sql for key, sql in declared.items()))
    require({name for kind, name in actual_master if kind == 'trigger'} ==
            {name for kind, name in declared if kind == 'trigger'})
    for entity in database['entities']:
        table = identifier(entity['tableName'])
        info = rows(connection, f'PRAGMA table_info({table})')
        expected = []
        pk = entity['primaryKey']['columnNames']
        for cid, field in enumerate(entity['fields']):
            # FTS4 strips declared types and null constraints in SQLite's virtual
            # table metadata. Preserve source flags in Rust, do not invent parity.
            fts = 'ftsVersion' in entity
            expected.append([cid, field['columnName'], '' if fts else field['affinity'],
                             0 if fts else int(field.get('notNull', False)), field.get('defaultValue'),
                             pk.index(field['columnName']) + 1 if field['columnName'] in pk else 0])
        require(info == expected)
        actual_fk = rows(connection, f'PRAGMA foreign_key_list({table})')
        expected_fk = []
        for fid, fk in enumerate(reversed(entity.get('foreignKeys', []))):
            for seq, (local, referenced) in enumerate(zip(fk['columns'], fk['referencedColumns'], strict=True)):
                expected_fk.append([fid, seq, fk['table'], local, referenced,
                                    fk['onUpdate'], fk['onDelete'], 'NONE'])
        require(actual_fk == expected_fk)
        indices = rows(connection, f'PRAGMA index_list({table})')
        require({i[1] for i in indices if i[3] == 'c'} == {i['name'] for i in entity.get('indices', [])})
        for index in entity.get('indices', []):
            actual = next(i for i in indices if i[1] == index['name'])
            require(actual[2:] == [int(index['unique']), 'c', 0])
            xinfo = rows(connection, f'PRAGMA index_xinfo({identifier(index["name"])})')
            key_rows = [i for i in xinfo if i[5] == 1]
            orders = index['orders'] or ['ASC'] * len(index['columnNames'])
            require([i[2] for i in key_rows] == index['columnNames'])
            require([i[3] for i in key_rows] == [int(o == 'DESC') for o in orders])
            require(all(i[4] == 'BINARY' for i in key_rows))
    require(rows(connection, 'SELECT id,identity_hash FROM room_master_table ORDER BY id') ==
            [[42, database['identityHash']]])


def catalog(connection: sqlite3.Connection, database: dict[str, Any]) -> dict[str, Any]:
    """Freeze actual complete master catalog and typed PRAGMA results."""
    tables = {}
    for entity in database['entities']:
        table = entity['tableName']
        indices = rows(connection, f'PRAGMA index_list({identifier(table)})')
        tables[table] = {'table_info': rows(connection, f'PRAGMA table_info({identifier(table)})'),
                         'index_list': indices,
                         'index_xinfo': {i[1]: rows(connection, f'PRAGMA index_xinfo({identifier(i[1])})') for i in indices},
                         'foreign_key_list': rows(connection, f'PRAGMA foreign_key_list({identifier(table)})')}
    return {'tables': tables, 'sqlite_master': rows(connection,
                'SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name'),
            'user_version': connection.execute('PRAGMA user_version').fetchone()[0],
            'master_identity': rows(connection, 'SELECT id,identity_hash FROM room_master_table ORDER BY id'),
            'foreign_keys': connection.execute('PRAGMA foreign_keys').fetchone()[0]}


def current_behavior(connection):
    """True v24 fields only; actual typed rows, FTS sync and FK preservation."""
    for gid, title in [(11, 'alphaterm'), (22, 'preserveterm'), (33, 'cascadeprobe')]:
        connection.execute('INSERT INTO GALLERIES (GID,TOKEN,TITLE,TITLE_JPN,CATEGORY,RATING,FAVORITE_SLOT) '
                           'VALUES (?,?,?,?,?,?,?)', (gid, 'synthetic', title, None, 0, 4.25, -1))
    connection.executemany('INSERT INTO PROGRESS (GID,PAGE) VALUES (?,?)', [(11, -2147483648), (22, 2147483647), (33, 0)])
    connection.executemany('INSERT INTO DOWNLOAD_LABELS (LABEL,POSITION,_id) VALUES (?,?,?)',
                           [('synthetic-label', 0, 1), ('unrelated-label', 1, 2)])
    connection.executemany('INSERT INTO DOWNLOADS (GID,STATE,LEGACY,TIME,LABEL) VALUES (?,?,?,?,?)',
                           [(11, 0, 0, -9223372036854775808, 'synthetic-label'),
                            (22, 0, 0, 9223372036854775807, 'unrelated-label')])
    connection.executemany('INSERT INTO DOWNLOAD_ARTISTS (GID,ARTIST) VALUES (?,?)', [(11, 'synthetic-artist'), (22, 'unrelated-artist')])
    def match(term):
        return [r[0] for r in connection.execute('SELECT docid FROM GALLERIES_FTS WHERE GALLERIES_FTS MATCH ? ORDER BY docid', (term,))]
    def logical():
        return {table: rows(connection, query) for table, query in {
            'GALLERIES': 'SELECT * FROM GALLERIES ORDER BY 1,2',
            'PROGRESS': 'SELECT * FROM PROGRESS ORDER BY 1,2',
            'DOWNLOAD_LABELS': 'SELECT * FROM DOWNLOAD_LABELS ORDER BY 1,2',
            'DOWNLOADS': 'SELECT * FROM DOWNLOADS ORDER BY 1,2',
            'DOWNLOAD_ARTISTS': 'SELECT * FROM DOWNLOAD_ARTISTS ORDER BY 1,2',
        }.items()}
    result = {'seeded': logical(), 'fts_insert': match('alphaterm')}
    require(result['fts_insert'] == [11] and match('preserveterm') == [22])
    connection.execute('UPDATE GALLERIES SET TITLE=?,SIMPLE_TAGS=? WHERE GID=11', ('betaterm', 'tagprobe'))
    result['fts_update'] = {'old': match('alphaterm'), 'new': match('betaterm'), 'tag': match('tagprobe')}
    require(result['fts_update'] == {'old': [], 'new': [11], 'tag': [11]})
    connection.execute('UPDATE DOWNLOAD_LABELS SET LABEL=? WHERE _id=1', ('renamed-label',))
    result['label_rename'] = rows(connection, 'SELECT GID,LABEL FROM DOWNLOADS ORDER BY GID')
    require(result['label_rename'] == [[11, 'renamed-label'], [22, 'unrelated-label']])
    connection.execute('DELETE FROM DOWNLOAD_LABELS WHERE _id=1')
    result['label_delete'] = rows(connection, 'SELECT GID,LABEL FROM DOWNLOADS ORDER BY GID')
    require(result['label_delete'] == [[11, None], [22, 'unrelated-label']])
    # NO ACTION gallery reference must reject without losing rows.
    connection.execute('SAVEPOINT fk_probe')
    try:
        connection.execute('DELETE FROM GALLERIES WHERE GID=22')
    except sqlite3.IntegrityError:
        connection.execute('ROLLBACK TO fk_probe')
    else:
        require(False)
    connection.execute('RELEASE fk_probe')
    result['gallery_no_action_rejected'] = True
    connection.execute('DELETE FROM DOWNLOADS WHERE GID=11')
    result['artists_cascade'] = rows(connection, 'SELECT GID,ARTIST FROM DOWNLOAD_ARTISTS ORDER BY GID,ARTIST')
    require(result['artists_cascade'] == [[22, 'unrelated-artist']])
    connection.execute('DELETE FROM GALLERIES WHERE GID=11')
    connection.execute('DELETE FROM GALLERIES WHERE GID=33')
    result['fts_delete'] = {'deleted': match('betaterm'), 'unrelated': match('preserveterm'), 'cascade': match('cascadeprobe')}
    require(result['fts_delete'] == {'deleted': [], 'unrelated': [22], 'cascade': []})
    result['final'] = logical()
    require(result['final']['PROGRESS'] == [[22, 2147483647]])
    for table, original in result['seeded'].items():
        key_column = 2 if table == 'DOWNLOAD_LABELS' else 0
        key = 2 if table == 'DOWNLOAD_LABELS' else 22
        unrelated = [r for r in original if r[key_column] == key]
        require(len(unrelated) == 1)
        require([r for r in result['final'][table] if r[key_column] == key] == unrelated)
    return result


def replay(source: dict[str, Any], statements: list[str] | None = None, version: int | None = None) -> dict[str, Any]:
    """Execute SQLite memory DB, never open supplied paths or real data."""
    database = source['database']
    with sqlite3.connect(':memory:') as connection:
        connection.execute('PRAGMA foreign_keys=ON')
        for statement in ddl(database) if statements is None else statements:
            connection.execute(statement)
        chosen_version = source['version'] if version is None else version
        require(type(chosen_version) is int and chosen_version in VERSIONS[source['name']])
        connection.execute(VERSION_PRAGMAS[chosen_version])
        result = catalog(connection, database)
        require(result['user_version'] == source['version'] and result['foreign_keys'] == 1)
        if source['name'] == 'EhDatabase' and source['version'] == 24:
            result['behavior'] = current_behavior(connection)
        verify_intent(connection, database)
        require(rows(connection, 'PRAGMA integrity_check') == [['ok']])
        require(rows(connection, 'PRAGMA foreign_key_check') == [])
        result['integrity_check'] = [['ok']]
        result['foreign_key_check'] = []
        return result


def fixtures(retained: list[dict[str, Any]]) -> dict[str, Any]:
    """Actual host results; no Room execution implied."""
    return {'provenance': 'Python stdlib SQLite synthetic memory replay; not Room or device',
            'sqlite_version': sqlite3.sqlite_version,
            'schemas': [{'name': s['name'], 'version': s['version'], 'source_sha256': s['source_sha256'],
                         'results': replay(s)} for s in retained]}


def artifacts(root: Path, retained: list[dict[str, Any]]) -> dict[str, bytes]:
    rust = rust_data(retained)
    fixture = encoded(fixtures(retained))
    hashes = {'exporter_sha256': digest((root / LANE / 'export.py').read_bytes()),
              'source_inventory_sha256': digest((root / LANE / 'source-hashes.json').read_bytes()),
              'rust_data_sha256': digest(rust), 'fixture_sha256': digest(fixture)}
    return {'database_schema_data.rs': rust, 'fixtures.json': fixture, 'artifact-hashes.json': encoded(hashes)}


def check(root=ROOT):
    """Drift checks plus real compiled-Rust DDL against independent source."""
    retained = sources(root)
    expected = artifacts(root, retained)
    require((root / 'native/src/database_schema_data.rs').read_bytes() == expected['database_schema_data.rs'])
    for name in ('fixtures.json', 'artifact-hashes.json'):
        require((root / LANE / name).read_bytes() == expected[name])
    emitted = load((root / BUILD / 'rust-ddl.json').read_bytes())
    require(type(emitted) is list and len(emitted) == len(retained))
    for source, rust in zip(retained, emitted, strict=True):
        require(set(rust) == {'name', 'version', 'identity', 'source_sha256', 'ddl', 'database'})
        require({k: rust[k] for k in ('name', 'version', 'identity', 'source_sha256')} ==
                {k: source[k] for k in ('name', 'version', 'identity', 'source_sha256')})
        require(rust['database'] == normalized_database(source['database']))
        # No equality shortcut on SQL hashes: execute actual Rust-emitted DDL
        # and compare complete catalog and seeded logical behavior.
        require(replay(source, rust['ddl'], rust['version']) == replay(source))


def normalized_database(database: dict[str, Any]) -> dict[str, Any]:
    """Defaults match typed Rust representation, not alternate SQL semantics."""
    result = {'version': database['version'], 'identityHash': database['identityHash'],
              'setupQueries': database['setupQueries'], 'entities': []}
    for original in database['entities']:
        entity = dict(original)
        entity.setdefault('indices', [])
        entity.setdefault('foreignKeys', [])
        entity.setdefault('ftsVersion', None)
        entity.setdefault('ftsOptions', None)
        entity.setdefault('contentSyncTriggers', [])
        entity['fields'] = [dict(f, notNull=f.get('notNull', False), defaultValue=f.get('defaultValue')) for f in original['fields']]
        result['entities'].append(entity)
    return result


def main(argv=None, root=ROOT):
    """No user-selected paths, no implicit generation, generic diagnostics."""
    argv = sys.argv[1:] if argv is None else argv
    if argv not in (['--check'], ['--generate']):
        sys.stderr.write(FAILURE)
        return 1
    try:
        if argv == ['--generate']:
            candidate = root / BUILD / 'candidate'
            candidate.mkdir(parents=True, exist_ok=True)
            for name, raw in artifacts(root, sources(root)).items():
                target = candidate / name
                # Explicit candidates never overwrite even earlier candidates.
                with target.open('xb') as output:
                    output.write(raw)
            print('Schema candidates generated; frozen evidence unchanged')
        else:
            check(root)
            print('Schema drift and 23-version SQLite differential passed')
        return 0
    except (OSError, ValueError, TypeError, KeyError, AttributeError, IndexError,
            RecursionError, OverflowError, sqlite3.Error, subprocess.SubprocessError):
        sys.stderr.write(FAILURE)
        return 1


if __name__ == '__main__':
    sys.exit(main())
