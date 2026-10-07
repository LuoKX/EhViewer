#!/usr/bin/env python3
"""Bound pinned inputs, synthetic replay, and owner-only coverage; redact failures."""
import hashlib
import json
import pathlib
import re
import sys

FAILURE = "FAIL: JVM UTF8 evidence unavailable, drifted, or malformed"
SNAPSHOT = pathlib.Path(__file__).with_name("historical-snapshot.json")
GOLDEN_SIZE = 5174663
GOLDEN_CASES = 258862
GOLDEN_HASH = "6ad30e1b16a84cc3a0e4de263fd6e878c9ffa55a32d5f483a02d5b8797a0fc42"


def bounded(path, maximum):
    """Read at most bound plus sentinel; reject oversized evidence."""
    with pathlib.Path(path).open("rb") as stream:
        result = stream.read(maximum + 1)
    if len(result) > maximum:
        raise ValueError
    return result


def unique_pairs(pairs):
    """Reject ambiguous JSON duplicate keys."""
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError
        result[key] = value
    return result


def reject_constant(_):
    """Reject JSON nonfinite extensions."""
    raise ValueError


def document(path, maximum=2097152):
    """Load bounded strict JSON."""
    try:
        return json.loads(bounded(path, maximum), object_pairs_hook=unique_pairs, parse_constant=reject_constant)
    except (OSError, ValueError, RecursionError):
        raise ValueError from None


def digest(path):
    """Hash regular frozen input with bounded read; never print path."""
    return hashlib.sha256(bounded(path, 536870912)).hexdigest()


def parse_golden(data, expected_cases):
    """Check complete binary record shape and independent UTF16/canonical relation."""
    if len(data) > GOLDEN_SIZE or data[:4] != b"JU01":
        raise ValueError
    position = 4

    def take(length):
        nonlocal position
        if position + length > len(data):
            raise ValueError
        result = data[position:position + length]
        position += length
        return result

    count = 0
    while True:
        length = int.from_bytes(take(1), "big")
        if length == 255:
            if int.from_bytes(take(4), "big") != count or count != expected_cases or position != len(data):
                raise ValueError
            return count
        if length > 64 or count >= expected_cases:
            raise ValueError
        take(length)
        units = int.from_bytes(take(1), "big")
        if units > 64:
            raise ValueError
        utf16 = take(units * 2)
        canonical_length = int.from_bytes(take(2), "big")
        if canonical_length > 192:
            raise ValueError
        canonical = take(canonical_length)
        if utf16.decode("utf-16-be").encode("utf-8") != canonical:
            raise ValueError
        count += 1


def replay(path):
    """Bind exact frozen JVM replay before accepting its shape."""
    data = bounded(path, GOLDEN_SIZE)
    if len(data) != GOLDEN_SIZE or hashlib.sha256(data).hexdigest() != GOLDEN_HASH:
        raise ValueError
    return parse_golden(data, GOLDEN_CASES)


def frozen(path):
    """Reject altered golden digest/count/size/family schemas before replay."""
    result = document(path, 65536)
    if not isinstance(result, dict) or set(result) != {"format", "byte_length", "cases", "sha256", "record", "trailer", "proof_scope", "families"}:
        raise ValueError
    if result["format"] != "JU01" or type(result["byte_length"]) is not int or result["byte_length"] != GOLDEN_SIZE or type(result["cases"]) is not int or result["cases"] != GOLDEN_CASES or result["sha256"] != GOLDEN_HASH:
        raise ValueError
    if any(not isinstance(result[key], str) or not result[key] for key in ["record", "trailer", "proof_scope"]):
        raise ValueError
    families = result["families"]
    expected = {"empty": 1, "single_bytes_exhaustive": 256, "two_bytes_exhaustive": 65536, "three_byte_leads_all_second_boundary_third": 40960, "four_byte_leads_all_second_boundary_truncated_third": 12800, "four_byte_leads_all_second_boundary_third_fourth": 128000, "all_encoded_surrogates": 2048, "adjacent_ascii_error_multibyte_extreme_triples": 9261}
    if not isinstance(families, dict) or families != expected or any(type(value) is not int for value in families.values()) or sum(families.values()) != GOLDEN_CASES:
        raise ValueError


def original_inputs(root, snapshot):
    """Validate archived scope; bind only 77 original parser/fixture inputs live.

    Other historical entries record worker isolation, not current application
    identity. Later accepted Rust modules/checkers must not rebind that history.
    """
    if not isinstance(snapshot, dict) or snapshot.get("tracked_base") != "af7c40ea9":
        raise ValueError
    entries = snapshot.get("sha256")
    if not isinstance(entries, dict) or len(entries) != 92:
        raise ValueError
    selected = {
        "native/parity/preferences/artifact-sha256.txt",
        "native/parity/preferences/Oracle.java",
        "native/parity/preferences/javap.txt",
        "native/parity/preferences/javap-map-entry.txt",
        "native/parity/preferences/javap-serializer.txt",
    }
    count = 0
    for name, expected in entries.items():
        if not isinstance(name, str) or not isinstance(expected, str):
            raise ValueError
        path = pathlib.Path(name)
        if path.is_absolute() or ".." in path.parts or not re.fullmatch("[a-f0-9]{64}", expected):
            raise ValueError
        if name.startswith("native/parity/preferences/goldens/") or name in selected:
            if digest(root / path) != expected:
                raise ValueError
            count += 1
    if count != 77:
        raise ValueError
    return count


def bind(root, manifest_path):
    """Bind actual owner/JVM/parser inputs and immutable historical provenance."""
    manifest = document(manifest_path)
    if not isinstance(manifest, dict) or set(manifest) != {"files", "snapshot_sha256"}:
        raise ValueError
    files = manifest["files"]
    if not isinstance(files, dict) or not files:
        raise ValueError
    for name, expected in files.items():
        if not isinstance(name, str) or not isinstance(expected, str) or not re.fullmatch("[a-f0-9]{64}", expected):
            raise ValueError
        path = pathlib.Path(name)
        if ".." in path.parts or digest(path if path.is_absolute() else root / path) != expected:
            raise ValueError
    if digest(SNAPSHOT) != manifest["snapshot_sha256"]:
        raise ValueError
    original_inputs(root, document(SNAPSHOT))
    if bounded(root / "native/src/lib.rs", 65536).splitlines().count(b"pub mod jvm_utf8;") != 1:
        raise ValueError
    required = {"native/src/jvm_utf8.rs", "native/tests/jvm_utf8.rs", *["native/parity/jvm-utf8/" + name for name in ["Oracle.java", "evidence.py", "test_evidence.py", "verify.py", "README.md", "integration-proposal.md", "frozen-jvm.json", "historical-snapshot.json"]]}
    jdk = pathlib.Path("/usr/lib/jvm/java-25-openjdk-amd64")
    required.update(str(jdk / name) for name in ["bin/java", "bin/javac", "bin/javap", "lib/modules", "lib/server/libjvm.so", "release"])
    required.update(line.split("  ", 1)[1] for line in (root / "native/parity/preferences/artifact-sha256.txt").read_text().splitlines())
    if not required.issubset(files):
        raise ValueError
    frozen(root / "native/parity/jvm-utf8/frozen-jvm.json")
    return len(files)


def coverage(path, source):
    """Apply 95% gates to actual owner source, excluding tests and other owners."""
    export = document(path, 67108864)
    if not isinstance(export, dict) or export.get("type") != "llvm.coverage.json.export" or len(export["data"]) != 1:
        raise ValueError
    data = export["data"][0]
    owner_files = [entry for entry in data["files"] if pathlib.Path(entry["filename"]).resolve() == source.resolve()]
    if len(owner_files) != 1:
        raise ValueError
    lines = owner_files[0]["summary"]["lines"]
    total, covered = lines["count"], lines["covered"]
    if type(total) is not int or type(covered) is not int or not 0 <= covered <= total or total == 0:
        raise ValueError
    source_lines = len(bounded(source, 65536).splitlines())
    if total > source_lines:
        raise ValueError
    functions = {}
    for function in data["functions"]:
        if pathlib.Path(function["filenames"][0]).resolve() != source.resolve():
            continue
        region = tuple(function["regions"][0][:4])
        count = function["count"]
        if len(region) != 4 or any(type(n) is not int or n < 0 for n in region) or not 1 <= region[0] <= region[2] <= source_lines or type(count) is not int or count < 0:
            raise ValueError
        functions[region] = max(functions.get(region, 0), count)
    if not functions or covered * 100 < total * 95 or sum(bool(n) for n in functions.values()) * 100 < len(functions) * 95:
        raise ValueError
    return covered, total, sum(bool(n) for n in functions.values()), len(functions)


def main(arguments=None):
    """Fixed generic diagnostics for every malformed/read/source-drift failure."""
    arguments = sys.argv[1:] if arguments is None else arguments
    try:
        root = pathlib.Path(__file__).resolve().parents[3]
        if len(arguments) == 2 and arguments[0] == "bind":
            count = bind(root, pathlib.Path(arguments[1]))
            print(f"Pinned JVM/source identities passed: {count} pins + 77 live parser/fixture inputs; 92 archived identities")
        elif len(arguments) == 2 and arguments[0] == "replay":
            print(f"Frozen synthetic JVM replay passed: {replay(arguments[1])} cases")
        elif len(arguments) == 2 and arguments[0] == "coverage":
            covered, total, hit, functions = coverage(arguments[1], root / "native/src/jvm_utf8.rs")
            print(f"Owner lines: {covered}/{total} = {100 * covered / total:.2f}%")
            print(f"Owner functions/closures: {hit}/{functions} = {100 * hit / functions:.2f}%")
        else:
            raise ValueError
        return 0
    except (OSError, ValueError, KeyError, IndexError, TypeError, OverflowError, RecursionError, AttributeError):
        print(FAILURE, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
