#!/usr/bin/env python3
"""Owner-only filesystem coverage; malformed/missing evidence never echoes input."""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def scopes():
    """Exclude codec and every nested cfg(test) module from filesystem gates."""
    result = []
    for name in ["preferences", "preference_lock"]:
        source = ROOT / "native/src" / f"{name}.rs"
        text = source.read_text().splitlines()
        start = text.index("pub struct FixtureStore {") + 1 if name == "preferences" else 1
        tests = []
        for index, line in enumerate(text):
            if line.strip() != "mod tests {":
                continue
            indent = line[: len(line) - len(line.lstrip())]
            end = next(j + 1 for j in range(index + 1, len(text)) if text[j] == indent + "}")
            tests.append((index + 1, end))
        if name == "preference_lock":
            # Standalone cfg(test) accessor is not part of production owner code.
            helper = text.index("        pub(crate) fn name(&self) -> &str {")
            if text[helper - 1].strip() != "#[cfg(test)]":
                raise ValueError
            end = next(j + 1 for j in range(helper + 1, len(text)) if text[j] == "        }")
            tests.append((helper, end))
        result.append((name, source.resolve(), start, len(text), tests))
    return result


def main(arguments=None):
    """Apply separate 95% line/function gates to both filesystem owner slices."""
    arguments = sys.argv[1:] if arguments is None else arguments
    try:
        if len(arguments) != 3:
            raise ValueError
        export = json.loads(Path(arguments[0]).read_text())["data"][0]
        metrics = []
        for (name, source, start, end, tests), lines_path in zip(scopes(), arguments[1:], strict=True):
            def owner(line, start=start, end=end, tests=tests):
                return start <= line <= end and not any(a <= line <= b for a, b in tests)
            functions = {}
            for function in export["functions"]:
                if Path(function["filenames"][0]).resolve() != source:
                    continue
                region = tuple(function["regions"][0][:4])
                count = function["count"]
                if len(region) != 4 or any(type(v) is not int or v < 0 for v in region) or type(count) is not int or count < 0:
                    raise ValueError
                if owner(region[0]):
                    functions[region] = max(functions.get(region, 0), count)
            lines = {}
            for line in Path(lines_path).read_text().splitlines():
                match = re.match(r"\s*(\d+)\|\s*([0-9]+(?:\.[0-9]+)?[kMG]?)\|", line)
                if match and owner(int(match[1])):
                    covered = any(digit in "123456789" for digit in match[2])
                    lines[int(match[1])] = max(lines.get(int(match[1]), False), covered)
            for kind, values in [("functions", functions), ("lines", lines)]:
                if not values:
                    raise ValueError
                hit = sum(bool(value) for value in values.values())
                metrics.append((name, kind, hit, len(values), 100 * hit / len(values)))
    except (OSError, ValueError, KeyError, IndexError, TypeError, OverflowError, StopIteration):
        print("FAIL: filesystem coverage evidence unavailable or malformed", file=sys.stderr)
        return 1
    for name, kind, hit, total, percent in metrics:
        print(f"{name} owner {kind}: {hit}/{total} = {percent:.2f}%")
    if any(percent < 95 for _, _, _, _, percent in metrics):
        print("FAIL: filesystem owner 95% gate failed", file=sys.stderr)
        return 1
    print("Host filesystem owner coverage; no Android/device durability inference.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
