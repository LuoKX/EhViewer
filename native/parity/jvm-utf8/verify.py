#!/usr/bin/env python3
"""Private offline focused checks; failed tool output never escapes diagnostics."""
import contextlib
import json
import os
import pathlib
import subprocess
import sys

import evidence

APPROVED_BUILD = pathlib.Path(__file__).resolve().parents[2] / "build/jvm-utf8"
PINNED_JAVA = "/usr/lib/jvm/java-25-openjdk-amd64/bin/java"
PINNED_JAVAC = "/usr/lib/jvm/java-25-openjdk-amd64/bin/javac"
FAILURE = "FAIL: JVM UTF8 validation unavailable or failed"


def run_tool(command, records, environment, output=None):
    """Record exact commands/results; discard potentially sensitive failed output."""
    try:
        result = subprocess.run(command, capture_output=True, env=environment, timeout=180, check=False)
    except (OSError, ValueError, subprocess.SubprocessError):
        records.append({"command": command, "result": "failed", "summary": "tool unavailable or failed"})
        raise ValueError from None
    records.append({"command": command, "result": "passed" if result.returncode == 0 else "failed", "summary": "focused check"})
    if result.returncode != 0:
        raise ValueError
    if output is not None:
        output.write_bytes(result.stdout)
    return result.stdout


def oracle_negative(command, records, environment):
    """Require real Java failure to expose only fixed redacted diagnostic."""
    result = subprocess.run(command, capture_output=True, env=environment, timeout=30, check=False)
    passed = result.returncode == 1 and result.stdout == b"" and result.stderr == b"FAIL: synthetic JVM oracle unavailable or malformed\n"
    records.append({"command": command, "result": "passed" if passed else "failed", "summary": "expected generic Java failure"})
    if not passed:
        raise ValueError


def main():
    """Validate actual source, frozen oracle, owner coverage, and Android metadata."""
    records = []
    build = None
    try:
        root = pathlib.Path(__file__).resolve().parents[3]
        base = root / "native/parity/jvm-utf8"
        candidate = pathlib.Path(os.environ.get("JVM_UTF8_BUILD", str(base / "build"))).resolve()
        if candidate != base / "build" and not candidate.is_relative_to(APPROVED_BUILD):
            raise ValueError
        # Rejected paths must never become failure-ledger destinations.
        build = candidate
        build.mkdir(parents=True, exist_ok=True)
        os.chdir(root)
        environment = dict(os.environ, TMPDIR=str(build), JVM_UTF8_BUILD=str(build), CARGO_BUILD_JOBS="2", CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(build / "target"))
        # Prevent installed JVM's default shared perfdata/temp writes outside lane.
        environment.pop("JAVA_TOOL_OPTIONS", None)
        environment.pop("_JAVA_OPTIONS", None)
        environment.pop("JDK_JAVA_OPTIONS", None)
        pins = evidence.bind(root, base / "pins.json")
        print(f"Pinned artifacts/source passed: {pins} pins + 77 live parser/fixture inputs; 92 archived identities")
        proto, external = [line.split("  ", 1)[1] for line in (root / "native/parity/preferences/artifact-sha256.txt").read_text().splitlines()[:2]]
        classpath = os.pathsep.join([proto, external])
        run_tool([PINNED_JAVAC, "-J-XX:-UsePerfData", f"-J-Djava.io.tmpdir={build}", "-cp", classpath, "-d", str(build), str(base / "Oracle.java")], records, environment)
        run_tool([PINNED_JAVA, "-XX:-UsePerfData", f"-Djava.io.tmpdir={build}", "-Xmx256m", "--sun-misc-unsafe-memory-access=allow", "-cp", os.pathsep.join([str(build), classpath]), "Oracle", str(build / "oracle.bin"), str(root / "native/parity/preferences/goldens")], records, environment)
        count = evidence.replay(build / "oracle.bin")
        print(f"Frozen real generated PreferenceMap/JVM replay passed: {count} cases")
        oracle_command = [PINNED_JAVA, "-XX:-UsePerfData", f"-Djava.io.tmpdir={build}", "--sun-misc-unsafe-memory-access=allow", "-cp", os.pathsep.join([str(build), classpath]), "Oracle"]
        for arguments in [[], [str(build / "negative.bin"), str(build / "missing-synthetic")], [str(build), str(root / "native/parity/preferences/goldens")]]:
            oracle_negative([*oracle_command, *arguments], records, environment)
        print("Three real Java argument/read/write negative checks passed")
        flags = ["--edition=2024", "-Dmissing_docs", "-Dunsafe_code", "-Drust_2018_idioms", "-Dunused_lifetimes"]
        source = "native/src/jvm_utf8.rs"
        tests = "native/tests/jvm_utf8.rs"
        run_tool(["rustfmt", "--check", "--edition", "2024", source, tests], records, environment)
        strict = [*flags, "-Dclippy::all", "-Dclippy::pedantic"]
        run_tool(["clippy-driver", "--crate-type", "lib", *strict, source, "--out-dir", str(build)], records, environment)
        run_tool(["clippy-driver", "--test", *strict, tests, "-o", str(build / "tests")], records, environment)
        run_tool([str(build / "tests"), "--test-threads=1"], records, environment)
        print("Strict focused rustfmt/Clippy and five Rust tests passed")
        for target in ["aarch64-linux-android", "armv7-linux-androideabi", "x86_64-linux-android"]:
            run_tool(["rustc", "--crate-type", "lib", *flags, "--emit=metadata", "--target", target, source, "-o", str(build / f"jvm-utf8-{target}.rmeta")], records, environment)
        print("All three Android metadata targets passed; no device execution")
        for profile in build.glob("jvm-utf8-*.profraw"):
            profile.unlink()
        run_tool(["rustc", "--test", *flags, "-Cinstrument-coverage", tests, "-o", str(build / "coverage-tests")], records, environment)
        coverage_env = dict(environment, LLVM_PROFILE_FILE=str(build / "jvm-utf8-%p.profraw"))
        run_tool([str(build / "coverage-tests"), "--test-threads=1"], records, coverage_env)
        sysroot = pathlib.Path(run_tool(["rustc", "--print", "sysroot"], records, environment).decode().strip())
        llvm = sysroot / "lib/rustlib/x86_64-unknown-linux-gnu/bin"
        run_tool([str(llvm / "llvm-profdata"), "merge", "-sparse", *map(str, build.glob("jvm-utf8-*.profraw")), "-o", str(build / "coverage.profdata")], records, environment)
        run_tool([str(llvm / "llvm-cov"), "export", str(build / "coverage-tests"), f"-instr-profile={build / 'coverage.profdata'}"], records, environment, build / "coverage.json")
        run_tool([str(llvm / "llvm-cov"), "show", str(build / "coverage-tests"), f"-instr-profile={build / 'coverage.profdata'}"], records, environment, build / "coverage-lines.txt")
        covered, total, hit, functions = evidence.coverage(build / "coverage.json", root / source)
        summary = f"Owner lines: {covered}/{total} = {100 * covered / total:.2f}%\nOwner functions/closures: {hit}/{functions} = {100 * hit / functions:.2f}%\n"
        (build / "coverage-summary.txt").write_text(summary)
        print(summary, end="")
        run_tool([sys.executable, "-B", str(base / "test_evidence.py")], records, environment)
        print("Evidence/runner tests passed, including launcher, history-scope and output-boundary negatives")
        # Recheck after all executions: no drift or accidental old-golden refresh.
        evidence.bind(root, base / "pins.json")
        (build / "commands-results.json").write_text(json.dumps(records, indent=2) + "\n")
        print("Focused JVM UTF8 validation passed; preferences integration/Android behavior gates open")
        return 0
    except (OSError, ValueError, KeyError, IndexError, TypeError, OverflowError, RecursionError, AttributeError, UnicodeError, subprocess.SubprocessError):
        if build is not None and build.is_dir():
            with contextlib.suppress(OSError):
                (build / "commands-results.json").write_text(json.dumps(records, indent=2) + "\n")
        print(FAILURE, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
