#!/usr/bin/env bash
# Isolated std-only owner tests/lints. No Cargo, registry, SDK or global writes.
set -euo pipefail
cd "$(dirname "$0")/../../.."
build=native/build/secure-text
mkdir -p "$build"
PYTHONDONTWRITEBYTECODE=1 python3 native/parity/secure-text/check_source.py
PYTHONDONTWRITEBYTECODE=1 python3 native/parity/secure-text/test_source_binding.py
rustfmt --check --edition 2024 native/src/secure_text.rs native/tests/secure_text.rs
printf '%s\n' '//! Isolated secure-text test library.' '#[path = "../../src/secure_text.rs"]' 'pub mod secure_text;' >"$build/harness.rs"
flags=(--edition=2024 -Dmissing_docs -Dunsafe_code)
rustc "${flags[@]}" --crate-name eh_native --crate-type=rlib "$build/harness.rs" -o "$build/libeh_native.rlib"
rustc "${flags[@]}" --test native/tests/secure_text.rs --extern "eh_native=$build/libeh_native.rlib" -o "$build/tests"
"$build/tests"
clippy-driver "${flags[@]}" -Dclippy::all -Dclippy::pedantic --crate-name eh_native --crate-type=rlib "$build/harness.rs" -o "$build/libeh_native.rlib"
clippy-driver "${flags[@]}" -Dclippy::all -Dclippy::pedantic --test native/tests/secure_text.rs --extern "eh_native=$build/libeh_native.rlib" -o "$build/clippy-tests"
"$build/clippy-tests"
if [[ "${1:-}" == --coverage ]]; then
	tools="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"
	rustc "${flags[@]}" -Cinstrument-coverage --crate-name eh_native --crate-type=rlib "$build/harness.rs" -o "$build/libeh_native.rlib"
	rustc "${flags[@]}" -Cinstrument-coverage --test native/tests/secure_text.rs --extern "eh_native=$build/libeh_native.rlib" -o "$build/coverage-tests"
	LLVM_PROFILE_FILE="$PWD/$build/owner.profraw" "$build/coverage-tests"
	"$tools/llvm-profdata" merge -sparse "$build/owner.profraw" -o "$build/owner.profdata"
	"$tools/llvm-cov" report "$build/coverage-tests" --instr-profile="$build/owner.profdata" --ignore-filename-regex='/tests/|harness.rs' | tee "$build/coverage-summary.txt"
	"$tools/llvm-cov" export "$build/coverage-tests" --instr-profile="$build/owner.profdata" --ignore-filename-regex='/tests/|harness.rs' >"$build/coverage.json"
	python3 - <<'PY'
import json
from pathlib import Path

files = json.loads(Path('native/build/secure-text/coverage.json').read_text())['data'][0]['files']
assert len(files) == 1 and Path(files[0]['filename']).resolve() == Path('native/src/secure_text.rs').resolve()
summary = files[0]['summary']
for metric in ['lines', 'functions']:
    if summary[metric]['percent'] < 95:
        raise SystemExit('FAIL: secure-text owner coverage below 95%')
print('PASS: secure-text owner line/function coverage >=95%')
PY
fi
