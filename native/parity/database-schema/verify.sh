#!/usr/bin/env bash
# Focused std-only contract verification. No Cargo/dependency/SDK/cache writes.
# Fixed workspace build directory may point at operator-approved private scratch.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"
BUILD="$ROOT/native/build/database-schema"
mkdir -p "$BUILD"
export TMPDIR="$BUILD" PYTHONDONTWRITEBYTECODE=1 CARGO_BUILD_JOBS=2
RUN="$(mktemp -d "$BUILD/verify.XXXXXX")"
LLVM_BIN="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin"
LINTS=(-D missing_docs -D unsafe_code -D rust_2018_idioms -D unused_lifetimes)
CLIPPY=(-D clippy::all -D clippy::pedantic)

rustfmt --edition=2024 --check native/src/database_schema.rs native/src/database_schema_data.rs \
    native/tests/database_schema.rs native/parity/database-schema/rust_replay.rs
clippy-driver --edition=2024 --crate-name eh_database_schema --crate-type lib --emit metadata \
    native/src/database_schema.rs -o "$RUN/host.rmeta" "${LINTS[@]}" "${CLIPPY[@]}"
clippy-driver --edition=2024 --crate-type bin native/parity/database-schema/rust_replay.rs \
    -o "$RUN/rust-replay" "${LINTS[@]}" "${CLIPPY[@]}"
"$RUN/rust-replay" > "$BUILD/rust-ddl.json"
python3 native/parity/database-schema/export.py --check
clippy-driver --edition=2024 --test native/tests/database_schema.rs \
    -o "$RUN/schema-tests" "${LINTS[@]}" "${CLIPPY[@]}"
"$RUN/schema-tests"
python3 native/parity/database-schema/test_evidence.py

for target in aarch64-linux-android armv7-linux-androideabi x86_64-linux-android; do
    clippy-driver --edition=2024 --crate-name eh_database_schema --crate-type lib --emit metadata \
        --target "$target" native/src/database_schema.rs -o "$RUN/$target.rmeta" \
        "${LINTS[@]}" "${CLIPPY[@]}"
    echo "$target: std-only schema metadata passed; not device/runtime/full UI"
done

rustc --edition=2024 --test native/tests/database_schema.rs -C instrument-coverage \
    -C opt-level=0 -C debuginfo=1 -o "$RUN/coverage-tests" "${LINTS[@]}"
LLVM_PROFILE_FILE="$RUN/schema.profraw" "$RUN/coverage-tests"
"$LLVM_BIN/llvm-profdata" merge -sparse "$RUN/schema.profraw" -o "$RUN/schema.profdata"
"$LLVM_BIN/llvm-cov" export "$RUN/coverage-tests" -instr-profile="$RUN/schema.profdata" \
    > "$BUILD/coverage.json"
python3 native/parity/database-schema/coverage.py
python3 native/parity/database-schema/check_artifacts.py
printf '%s\n' 'Schema source/fixture/lint/host replay/negative/owner coverage/metadata gates passed'
