#!/usr/bin/env python3
"""Read-only drift gate for pinned cache artifacts and explicit bytecode fixtures."""
import argparse
import hashlib
import io
import json
import re
import subprocess
import tempfile
import zipfile
from pathlib import Path


class DriftError(Exception):
    """Source binding no longer matches; messages contain no source values."""


def digest(data):
    """Hash evidence without emitting source data."""
    return hashlib.sha256(data).hexdigest()


def require_digest(data, expected):
    """Fail closed on changed evidence."""
    if digest(data) != expected:
        raise DriftError("secure-text source drift")


def select_disassembly(output, sections):
    """Reject invalid/empty sections instead of hashing silently truncated proof."""
    lines = output.splitlines(keepends=True)
    instruction = re.compile(r"^\s*\d+:\s+\S", re.MULTILINE)
    if sections is None:
        if not instruction.search(output):
            raise DriftError("secure-text disassembly unavailable")
        return output
    if not isinstance(sections, list) or not sections:
        raise DriftError("invalid secure-text disassembly sections")
    selected = []
    for section in sections:
        if not isinstance(section, list) or len(section) != 2:
            raise DriftError("invalid secure-text disassembly section")
        start, end = section
        if (
            type(start) is not int
            or type(end) is not int
            or not 1 <= start <= end <= len(lines)
        ):
            raise DriftError("invalid secure-text disassembly bounds")
        body = "".join(lines[start - 1:end])
        if not instruction.search(body):
            raise DriftError("secure-text section lacks disassembly")
        selected.append(f"# javap lines {start}-{end}\n" + body)
    return "".join(selected)


def verify(root, cache, fixture=None):
    """Verify inputs, cached binaries, class bytes and regenerated javap sections."""
    fixture = fixture or root / "native/parity/secure-text"
    try:
        binding = json.loads((fixture / "source-binding.json").read_text())
    except (OSError, ValueError):
        raise DriftError("secure-text evidence unavailable or malformed") from None
    if not isinstance(binding, dict) or binding.get("schema_version") != 1:
        raise DriftError("unsupported secure-text binding schema")
    for entry in binding["inputs"]:
        require_digest((root / entry["path"]).read_bytes(), entry["sha256"])
    require_digest((fixture / "cases.tsv").read_bytes(), binding["cases_sha256"])
    # Private snapshot temp only; never Gradle cache, SDK or global temp files.
    build = root / "native/build/secure-text"
    build.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=build) as temporary:
        jar = Path(temporary) / "classes.jar"
        for artifact in binding["artifacts"]:
            aar = (cache / artifact["cache_relative_path"]).read_bytes()
            require_digest(aar, artifact["aar_sha256"])
            classes = zipfile.ZipFile(io.BytesIO(aar)).read("classes.jar")
            require_digest(classes, artifact["classes_jar_sha256"])
            jar.write_bytes(classes)
            archive = zipfile.ZipFile(io.BytesIO(classes))
            for entry in artifact["classes"]:
                name = entry["class_name"]
                require_digest(
                    archive.read(name.replace(".", "/") + ".class"),
                    entry["class_sha256"],
                )
                output = subprocess.check_output(
                    ["javap", "-c", "-p", "-constants", "-classpath", str(jar), name],
                    text=True,
                    stderr=subprocess.DEVNULL,
                )
                output = select_disassembly(output, entry["sections"])
                require_digest(output.encode(), entry["fixture_sha256"])
                require_digest(
                    (fixture / entry["fixture"]).read_bytes(), entry["fixture_sha256"]
                )


def main():
    """Run verification only; intentional source updates need separate review."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cache-root",
        type=Path,
        default=Path.home() / ".gradle/caches/modules-2/files-2.1",
    )
    arguments = parser.parse_args()
    try:
        verify(Path(__file__).resolve().parents[3], arguments.cache_root)
    except (DriftError, OSError, KeyError, ValueError, subprocess.CalledProcessError):
        print("FAIL: secure-text evidence unavailable or drifted")
        return 1
    print("PASS: pinned inputs, AARs, class bytes and javap fixtures match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
