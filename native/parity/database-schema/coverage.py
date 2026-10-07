#!/usr/bin/env python3
"""95% exact schema owner lines/functions from actual LLVM coverage JSON.

Never report input, paths or parser/OS messages. Static generated data contains
no executable functions; its exact source/fixture gate lives in export.py.
"""
import json
import posixpath
import sys
from pathlib import Path
from typing import Any

OWNER = 'native/src/database_schema.rs'
ROOT = Path(__file__).resolve().parents[3]
REPORT = Path('native/build/database-schema/coverage.json')
FAILURE = 'Schema coverage failed\n'


def check(report: dict[str, Any]) -> dict[str, float]:
    """Reject absent/empty/duplicate owner, malformed counts and subthreshold metrics."""
    files = [entry for data in report['data'] for entry in data['files']
             if posixpath.normpath(entry['filename'].replace('\\', '/')) == OWNER
             or posixpath.normpath(entry['filename'].replace('\\', '/')).endswith('/' + OWNER)]
    if len(files) != 1:
        raise ValueError('invalid coverage')
    percentages = {}
    for name in ('lines', 'functions'):
        metric = files[0]['summary'][name]
        count, covered = metric['count'], metric['covered']
        if type(count) is not int or type(covered) is not int or not 0 <= covered <= count or count == 0:
            raise ValueError('invalid coverage')
        percentages[name] = covered * 100 / count
        if percentages[name] < 95:
            raise ValueError('insufficient coverage')
    return percentages


def main(argv=None, root=ROOT):
    """Read fixed private report, never synthesize or refresh coverage."""
    argv = sys.argv[1:] if argv is None else argv
    if argv:
        sys.stderr.write(FAILURE)
        return 1
    try:
        result = check(json.loads((root / REPORT).read_bytes()))
    except (OSError, ValueError, TypeError, AttributeError, KeyError, IndexError, RecursionError):
        sys.stderr.write(FAILURE)
        return 1
    print(f"Schema owner lines={result['lines']:.2f}% functions={result['functions']:.2f}%; gate passed")
    return 0


if __name__ == '__main__':
    sys.exit(main())
