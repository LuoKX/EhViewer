#!/usr/bin/env bash
# Synthetic-only checks. No installs, Cargo, production paths, or cache writes.
set -euo pipefail
cd "$(dirname "$0")/../../.."
base=native/parity/preferences
build="$base/build"
mkdir -p "$build"
proto=/home/ngnl/.gradle/caches/modules-2/files-2.1/androidx.datastore/datastore-preferences-proto/1.2.1/5d062b556fb45141138c3d86284bbdcbe07cc770/datastore-preferences-proto-1.2.1.jar
external=/home/ngnl/.gradle/caches/modules-2/files-2.1/androidx.datastore/datastore-preferences-external-protobuf/1.2.1/200a891fd29602b4bcff8e197d243a372fb026d5/datastore-preferences-external-protobuf-1.2.1.jar
sha256sum -c "$base/artifact-sha256.txt"
sha256sum -c "$base/source-sha256.txt"
sha256sum -c "$base/changed-source-sha256.txt"
sha256sum -c "$base/evidence-sha256.txt"
python3 -B "$base/test_coverage_summary.py"
javac -cp "$proto:$external" -d "$build" "$base/Oracle.java"
java -cp "$build:$proto:$external" Oracle "$build/oracle-goldens"
for expected in "$base"/goldens/*; do cmp "$expected" "$build/oracle-goldens/$(basename "$expected")"; done
printf 'JVM golden replay: exact match\n'
flags=(--edition=2024 -Dmissing_docs -Dunsafe_code -Drust_2018_idioms -Dunused_lifetimes)
rustfmt --check --edition 2024 native/src/preferences.rs native/tests/preferences.rs
clippy-driver --crate-type lib "${flags[@]}" -Dclippy::all -Dclippy::pedantic native/src/preferences.rs --out-dir "$build"
clippy-driver --test "${flags[@]}" -Dclippy::all -Dclippy::pedantic native/tests/preferences.rs -o "$build/preferences-tests"
"$build/preferences-tests"
if rustc --crate-name stale_snapshot --edition=2024 "$base/stale-snapshot.rs.txt" -o "$build/stale-snapshot" 2>"$build/stale-snapshot.stderr"; then
	printf 'ERROR: stale snapshot replacement compiled\n' >&2
	exit 1
fi
grep -q 'expected `FixtureEdit.*found `Preferences`' "$build/stale-snapshot.stderr"
printf 'Stale snapshot compile-fail: expected type rejection\n'
for target in aarch64-linux-android armv7-linux-androideabi x86_64-linux-android; do
	rustc --crate-type lib "${flags[@]}" --emit=metadata --target "$target" native/src/preferences.rs -o "$build/preferences-$target.rmeta"
	printf 'Android metadata compile: %s passed (not device execution)\n' "$target"
done
sysroot=$(rustc --print sysroot)
llvm="$sysroot/lib/rustlib/x86_64-unknown-linux-gnu/bin"
rm -f "$build"/preferences-*.profraw
rustc --test "${flags[@]}" -Cinstrument-coverage native/tests/preferences.rs -o "$build/preferences-coverage-tests"
LLVM_PROFILE_FILE="$PWD/$build/preferences-%p.profraw" "$build/preferences-coverage-tests" --test-threads=1
"$llvm/llvm-profdata" merge -sparse "$build"/preferences-*.profraw -o "$build/preferences.profdata"
"$llvm/llvm-cov" export "$build/preferences-coverage-tests" -instr-profile="$build/preferences.profdata" >"$build/coverage.json"
"$llvm/llvm-cov" show "$build/preferences-coverage-tests" -instr-profile="$build/preferences.profdata" -ignore-filename-regex='(\.rustup/|native/tests/preferences.rs)' >"$build/coverage-lines.txt"
python3 "$base/coverage-summary.py" "$build/coverage.json" "$build/coverage-lines.txt" | tee "$build/coverage-summary.txt"
printf 'Focused preferences validation passed; full Cargo/Android runtime gates remain open.\n'
