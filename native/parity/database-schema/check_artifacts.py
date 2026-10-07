#!/usr/bin/env python3
"""Fail-closed exact inventory of schema seam sources and frozen host evidence.

No automatic hash refresh. Parent alone may rebind lib registration on integration.
Input/OS/parser errors never include contents or paths.
"""
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
LANE = Path('native/parity/database-schema')
ARTIFACTS = (
    'native/src/lib.rs',
    'native/src/database_schema.rs',
    'native/src/database_schema_data.rs',
    'native/tests/database_schema.rs',
    *(str(LANE / name) for name in (
        'README.md', 'export.py', 'rust_replay.rs', 'coverage.py', 'test_evidence.py',
        'verify.sh', 'check_artifacts.py', 'source-hashes.json', 'artifact-hashes.json',
        'fixtures.json', 'coverage-summary.json',
    )),
)
FAILURE = 'Schema artifacts failed\n'


def check(root=ROOT):
    """Require exact manifest set, complete digests and matching source bytes."""
    try:
        hashes = json.loads((root / LANE / 'artifact-sha256.json').read_bytes())
    except (OSError, ValueError, RecursionError):
        raise ValueError('invalid inventory') from None
    if type(hashes) is not dict or set(hashes) != set(ARTIFACTS):
        raise ValueError('invalid inventory')
    for path in ARTIFACTS:
        if hashes[path] != hashlib.sha256((root / path).read_bytes()).hexdigest():
            raise ValueError('artifact drift')


def main(argv=None, root=ROOT):
    """Only fixed inventory, generic diagnostics and no evidence writes."""
    argv = sys.argv[1:] if argv is None else argv
    if argv:
        sys.stderr.write(FAILURE)
        return 1
    try:
        check(root)
    except (OSError, ValueError, TypeError, AttributeError, KeyError, RecursionError):
        sys.stderr.write(FAILURE)
        return 1
    print('Schema source and frozen artifact inventory passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
