#!/usr/bin/env python3
"""Gate each P3 production owner independently; no cross-owner/test coverage borrowing."""
import re
import subprocess
from pathlib import Path

RUST_ROOT = Path(__file__).resolve().parents[1] / "app/src/main/rust"
OWNERS = (
    ("eh_domain", "file_rules", "crates/domain/src/file_rules.rs"),
    ("eh_domain", "text_spans", "crates/domain/src/text_spans.rs"),
    ("eh_archive", "production_zip", "crates/archive/src/production_zip.rs"),
    ("eh_archive", "production_tar", "crates/archive/src/production_tar.rs"),
    ("eh_archive", "archive_filters", "crates/archive/src/archive_filters.rs"),
    ("eh_codec_limits", "", "crates/codec_limits/src/lib.rs"),
    ("eh_image", "webp", "crates/image/src/webp.rs"),
    ("eh_ffi", "webp", "crates/ffi/src/webp.rs"),
)


def main():
    sources = sorted(RUST_ROOT.glob("crates/*/src/**/*.rs"))
    sources += sorted((RUST_ROOT / "src").rglob("*.rs"))
    sources += sorted((RUST_ROOT / "vendor").rglob("*.rs"))
    output = RUST_ROOT / "target/llvm-cov"
    output.mkdir(parents=True, exist_ok=True)
    for package, module, relative_path in OWNERS:
        owned = RUST_ROOT / relative_path
        excluded = "|".join(re.escape(str(path)) + "$" for path in sources if path != owned)
        name = module.replace("::", "-") if module else "decoder-limits"
        if module == "webp":
            name = f"{package}-{module}"
        selector = f"{module}::tests::" if module else "tests::"
        artifact = output / f"p3-{name}.lcov"
        subprocess.run(["cargo", "llvm-cov", "clean", "--workspace"], cwd=RUST_ROOT, check=True)
        subprocess.run([
            "cargo", "llvm-cov", "--package", package, "--lib",
            *(["--features", "jvm"] if package == "eh_ffi" else []),
            "--ignore-filename-regex", f"({excluded})",
            "--lcov", "--output-path", str(artifact),
            "--fail-under-lines", "95", "--fail-under-functions", "95",
            "--", selector,
        ], cwd=RUST_ROOT, check=True)
        records = artifact.read_text().split("end_of_record")
        measured = [record for record in records if "SF:" in record]
        if len(measured) != 1 or f"SF:{owned}" not in measured[0]:
            raise SystemExit(f"{module}: unexpected production coverage denominator")
        totals = dict(re.findall(r"^(LF|LH|FNF|FNH):(\d+)$", measured[0], re.MULTILINE))
        print(f"{name}: lines {totals['LH']}/{totals['LF']}, functions {totals['FNH']}/{totals['FNF']}", flush=True)


if __name__ == "__main__":
    main()
