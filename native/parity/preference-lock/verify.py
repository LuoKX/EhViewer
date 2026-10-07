#!/usr/bin/env python3
"""Redacted focused filesystem replay; private cached Cargo closure, no device run."""
import contextlib
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / "native/parity/preference-lock"
BUILD = BASE / "build"
COMMANDS = []


def run(command, label, *, cwd=ROOT, env=None, expected_failure=False):
    """Capture/discard diagnostics; never forward payload/path-bearing failures."""
    result = subprocess.run(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    COMMANDS.append({"command": command, "result": result.returncode, "label": label})
    if not expected_failure and result.returncode:
        raise RuntimeError(label)
    if not expected_failure:
        print(f"PASS: {label}")
    return result


def load_json(raw):
    """Normalize baseline/command parser failures without carrying input context."""
    try:
        return json.loads(raw)
    except (ValueError, RecursionError):
        raise ValueError("invalid evidence") from None


def source_gates():
    """Frozen codec/JVM and exact additive Cargo graph gates, not broad refresh."""
    baseline = load_json((BASE / "baseline-identities.json").read_text())
    owner = (ROOT / "native/src/preferences.rs").read_bytes()
    a = owner.index(b"/// Redacted error categories:")
    b = owner.index(b"/// Rename has committed new bytes;")
    if hashlib.sha256(owner[a:b]).hexdigest() != baseline["codec_sha256"]:
        raise ValueError
    bounds = [line for line in owner.decode().splitlines() if re.match(r"(pub )?const MAX_(BYTES|ENTRIES|SET_STRINGS|FIELDS|GROUP_DEPTH):", line)]
    if bounds != baseline["codec_bounds"]:
        raise ValueError
    for path, digest in baseline["parent92"]["sha256"].items():
        if ("/goldens/" in path or path.endswith(("Oracle.java", "javap.txt")) or "javap-" in path) and hashlib.sha256((ROOT / path).read_bytes()).hexdigest() != digest:
            raise ValueError
    artifacts = ROOT / "native/parity/preferences/artifact-sha256.txt"
    if hashlib.sha256(artifacts.read_bytes()).hexdigest() != baseline["parent92"]["sha256"]["native/parity/preferences/artifact-sha256.txt"]:
        raise ValueError
    for line in artifacts.read_text().splitlines():
        digest, path = line.split("  ", 1)
        if not path.startswith("/home/ngnl/.gradle/caches/modules-2/files-2.1/androidx.datastore/") or hashlib.sha256(Path(path).read_bytes()).hexdigest() != digest:
            raise ValueError
    old = baseline["native_lock_packages"]
    new = tomllib.loads((ROOT / "native/Cargo.lock").read_text())["package"]
    if len(old) != len(new):
        raise ValueError
    for previous, current in zip(old, new, strict=True):
        expected = dict(previous)
        if expected["name"] == "eh_native":
            expected["dependencies"] = sorted([*expected["dependencies"], "rustix 1.1.5"])
        if current != expected:
            raise ValueError
    source_manifests()


def source_manifests(root=None):
    """Consume source bindings; require lock source/test and forbid generated pyc."""
    root = ROOT if root is None else root
    required = {"native/src/preference_lock.rs", "native/tests/preference_lock.rs"}
    for manifest in ["native/parity/preferences/changed-source-sha256.txt", "native/parity/preferences/source-sha256.txt", "native/parity/preferences/evidence-sha256.txt", "native/parity/preference-lock/evidence-sha256.txt"]:
        bound = set()
        for line in (root / manifest).read_text().splitlines():
            digest, path = line.split("  ", 1)
            if not re.fullmatch(r"[a-f0-9]{64}", digest) or Path(path).is_absolute() or ".." in Path(path).parts or "__pycache__" in Path(path).parts or Path(path).suffix == ".pyc":
                raise ValueError
            if hashlib.sha256((root / path).read_bytes()).hexdigest() != digest:
                raise ValueError
            bound.add(path)
        if manifest.endswith("changed-source-sha256.txt") and not required.issubset(bound):
            raise ValueError


def verify():
    """Exercise host syscall/component semantics and cross-target compile only."""
    source_gates()
    print("PASS: frozen source/fixture/additive dependency gates")
    run([sys.executable, "-B", str(BASE / "test_evidence.py")], "redacted filesystem checker negatives")
    run([sys.executable, "-B", str(ROOT / "native/parity/preferences/test_coverage_summary.py")], "parent checker nine negatives")
    run([sys.executable, "-B", str(BASE / "prepare.py")], "private cached dependency preparation")
    env = dict(os.environ, CARGO_HOME=str(BUILD / "cargo-home"), CARGO_TARGET_DIR=str(BUILD / "target"), TMPDIR=str(BUILD / "tmp"), CARGO_BUILD_JOBS="2")
    harness = BUILD / "harness"
    run(["cargo", "generate-lockfile", "--offline"], "private frozen-closure lock initialization", cwd=harness, env=env)
    native = tomllib.loads((ROOT / "native/Cargo.lock").read_text())["package"]
    focused = tomllib.loads((harness / "Cargo.lock").read_text())["package"]
    for package in focused:
        if package["name"] == "preference_filesystem_harness":
            continue
        if not any(all(package.get(k) == n.get(k) for k in ["name", "version", "source", "checksum"]) for n in native):
            raise ValueError
    run(["rustfmt", "--check", "--edition", "2024", "--config", "skip_children=true", *[str(ROOT / p) for p in ["native/src/preferences.rs", "native/src/preference_lock.rs", "native/tests/preferences.rs", "native/tests/preference_lock.rs"]]], "focused formatting")
    run(["cargo", "clippy", "--offline", "--locked", "-j2", "--all-targets", "--", "-Dmissing_docs", "-Dunsafe_code", "-Drust_2018_idioms", "-Dunused_lifetimes", "-Dclippy::all", "-Dclippy::pedantic"], "focused strict library/test lints", cwd=harness, env=env)
    run(["cargo", "test", "--offline", "--locked", "-j2", "--tests"], "host descriptor/codec/runtime tests", cwd=harness, env=env)
    binaries = run(["cargo", "test", "--offline", "--locked", "-j2", "--tests", "--no-run", "--message-format=json"], "focused executable inventory", cwd=harness, env=env)
    bins = [j["executable"] for line in binaries.stdout.splitlines() if line.startswith(b"{") and (j := load_json(line)).get("executable")]
    for _ in range(10):
        for binary in bins:
            run([binary], "bounded parallel stress", cwd=harness, env=env)
    compile_failure(harness, env)
    for target in ["aarch64-linux-android", "armv7-linux-androideabi", "x86_64-linux-android"]:
        run(["cargo", "check", "--offline", "--locked", "-j2", "--lib", "--target", target], f"Android compile-only {target}", cwd=harness, env=env)
    oracle()
    coverage(harness, env)
    (BUILD / "commands.json").write_text(json.dumps(COMMANDS, indent=2) + "\n")
    print("PASS: isolated filesystem slice; Android device/cutover approval not implied")


def compile_failure(harness, env):
    """Compile unchanged stale-editor fixture through real dependency harness."""
    text = (ROOT / "native/parity/preferences/stale-snapshot.rs.txt").read_text()
    text = text.replace('#[path = "../../src/preferences.rs"]\nmod preferences;', 'use preference_filesystem_harness::preferences;')
    directory = harness / "src/bin"
    directory.mkdir(exist_ok=True)
    source = directory / "stale_snapshot.rs"
    source.write_text(text)
    try:
        result = run(["cargo", "check", "--offline", "--locked", "-j2", "--bin", "stale_snapshot"], "expected stale editor rejection", cwd=harness, env=env, expected_failure=True)
        if not result.returncode or not all(marker in result.stdout for marker in [b"E0308", b"FixtureEdit", b"Preferences"]):
            raise ValueError
    finally:
        source.unlink()
    print("PASS: expected stale editor type rejection")


def oracle():
    """Replay cached JVM oracle without altering any frozen golden byte."""
    prefix = "/home/ngnl/.gradle/caches/modules-2/files-2.1/androidx.datastore"
    proto = f"{prefix}/datastore-preferences-proto/1.2.1/5d062b556fb45141138c3d86284bbdcbe07cc770/datastore-preferences-proto-1.2.1.jar"
    external = f"{prefix}/datastore-preferences-external-protobuf/1.2.1/200a891fd29602b4bcff8e197d243a372fb026d5/datastore-preferences-external-protobuf-1.2.1.jar"
    classes = BUILD / "java"
    classes.mkdir(exist_ok=True)
    run(["javac", "-cp", f"{proto}:{external}", "-d", str(classes), str(ROOT / "native/parity/preferences/Oracle.java")], "cached JVM oracle compile")
    output = BUILD / "oracle"
    run(["java", "-cp", f"{classes}:{proto}:{external}", "Oracle", str(output)], "cached JVM oracle execution")
    for expected in (ROOT / "native/parity/preferences/goldens").iterdir():
        if expected.read_bytes() != (output / expected.name).read_bytes():
            raise ValueError
    print("PASS: every JVM golden unchanged")


def coverage(harness, env):
    """Fresh instrumentation; merge lexical aliases by owner, never mask codec."""
    env = dict(env, CARGO_TARGET_DIR=str(BUILD / "coverage-target"), RUSTFLAGS="-Cinstrument-coverage", LLVM_PROFILE_FILE=str(BUILD / "fresh-%p-%m.profraw"))
    for path in BUILD.glob("fresh-*.profraw"):
        path.unlink()
    run(["cargo", "test", "--offline", "--locked", "-j2", "--tests"], "fresh host instrumentation", cwd=harness, env=env)
    output = run(["cargo", "test", "--offline", "--locked", "-j2", "--tests", "--no-run", "--message-format=json"], "coverage executable inventory", cwd=harness, env=env)
    bins = [j["executable"] for line in output.stdout.splitlines() if line.startswith(b"{") and (j := load_json(line)).get("executable")]
    sysroot = run(["rustc", "--print", "sysroot"], "installed compiler identity").stdout.decode().strip()
    llvm = Path(sysroot) / "lib/rustlib/x86_64-unknown-linux-gnu/bin"
    profile = BUILD / "fresh.profdata"
    run([str(llvm / "llvm-profdata"), "merge", "-sparse", *[str(p) for p in BUILD.glob("fresh-*.profraw")], "-o", str(profile)], "fresh profile merge")
    common = [bins[0], *[f"--object={p}" for p in bins[1:]], f"-instr-profile={profile}"]
    export = run([str(llvm / "llvm-cov"), "export", *common], "owner coverage export")
    (BUILD / "fresh-coverage.json").write_bytes(export.stdout)
    text = run([str(llvm / "llvm-cov"), "show", *common, r"-ignore-filename-regex=(vendor|\.rustup|native/tests/(preferences|preference_lock)\.rs)"], "all lexical source aliases").stdout.decode()
    chunks = {"preferences": [], "preference_lock": []}
    current = None
    for line in text.splitlines():
        if line.startswith("/") and line.endswith(":"):
            name = Path(line[:-1]).stem
            current = name if name in chunks else None
        if current:
            chunks[current].append(line)
    for name, lines in chunks.items():
        (BUILD / f"fresh-{name}.txt").write_text("\n".join(lines) + "\n")
    paths = [str(BUILD / name) for name in ["fresh-coverage.json", "fresh-preferences.txt", "fresh-preference_lock.txt"]]
    result = run([sys.executable, "-B", str(BASE / "coverage.py"), *paths], "independent filesystem owner95 gates")
    (BUILD / "owner-summary.txt").write_bytes(result.stdout)
    # Checker output contains only fixed owner labels/counts, never raw evidence.
    print(result.stdout.decode(), end="")
    result = run([sys.executable, "-B", str(ROOT / "native/parity/preferences/coverage-summary.py"), *paths[:2]], "original preference owner95 gate")
    (BUILD / "codec-summary.txt").write_bytes(result.stdout)


def main():
    """Stop on failures with generic diagnostics; retain only command statuses."""
    try:
        verify()
    except (OSError, ValueError, KeyError, IndexError, TypeError, RuntimeError, UnicodeError, subprocess.SubprocessError):
        print("FAIL: focused filesystem verification unavailable or invalid", file=sys.stderr)
        with contextlib.suppress(OSError):
            (BUILD / "commands.json").write_text(json.dumps(COMMANDS, indent=2) + "\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
