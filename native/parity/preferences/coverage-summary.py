#!/usr/bin/env python3
"""Summarize owner code only; reject malformed evidence without payload output."""
import json
import pathlib
import re
import sys


def main(arguments=None):
    """Apply owner-only line/function gates; return redacted failure status."""
    arguments = sys.argv[1:] if arguments is None else arguments
    try:
        if len(arguments) != 2:
            raise ValueError
        root = pathlib.Path(__file__).resolve().parents[3]
        source = root / "native/src/preferences.rs"
        boundary = source.read_text().splitlines().index("mod tests {") + 1
        export = json.loads(pathlib.Path(arguments[0]).read_text())["data"][0]
        functions = {}
        for function in export["functions"]:
            if pathlib.Path(function["filenames"][0]).resolve() != source:
                continue
            region = tuple(function["regions"][0][:4])
            count = function["count"]
            if (
                len(region) != 4
                or any(type(value) is not int or value < 0 for value in region)
                or type(count) is not int
                or count < 0
            ):
                raise ValueError
            if region[0] >= boundary:
                continue
            # Generic instantiations count as one source function/closure.
            functions[region] = max(functions.get(region, 0), count)
        lines = {}
        for text in pathlib.Path(arguments[1]).read_text().splitlines():
            match = re.match(r"\s*(\d+)\|\s*([0-9]+(?:\.[0-9]+)?[kMG]?)\|", text)
            if match and int(match[1]) < boundary:
                covered = any(digit in "123456789" for digit in match[2])
                lines[int(match[1])] = max(lines.get(int(match[1]), False), covered)
        metrics = []
        for label, counts in [
            ("Owner source functions/closures (deduplicated)", functions),
            ("Owner executable lines", lines),
        ]:
            if not counts:
                raise ValueError
            covered = sum(bool(value) for value in counts.values())
            total = len(counts)
            metrics.append((label, covered, total, 100 * covered / total))
    except (OSError, ValueError, KeyError, IndexError, TypeError, OverflowError):
        print("FAIL: preference coverage evidence unavailable or malformed", file=sys.stderr)
        return 1

    for label, covered, total, percent in metrics:
        print(f"{label}: {covered}/{total} = {percent:.2f}%")
    if any(percent < 95 for _, _, _, percent in metrics):
        print("FAIL: owner-specific 95% gate failed", file=sys.stderr)
        return 1
    print(f"Scope: native/src/preferences.rs lines 1..{boundary - 1}; cfg(test) hooks remain, unit-test module excluded.")
    print("Coverage observes synthetic host tests, not Android execution or power-loss durability.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
