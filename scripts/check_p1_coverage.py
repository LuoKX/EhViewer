#!/usr/bin/env python3
"""Gate each P1 production owner independently; no cross-owner/test coverage borrowing."""
import re
import subprocess
from pathlib import Path

RUST_ROOT = Path(__file__).resolve().parents[1] / "app/src/main/rust"
OWNERS = (
    ("eh_net", "tag_translation", "crates/net/src/tag_translation.rs"),
    ("eh_net", "gallery::request", "crates/net/src/gallery/request.rs"),
    ("eh_domain", "gallery_enrichment", "crates/domain/src/gallery_enrichment.rs"),
    ("eh_domain", "recovery_eh_lookup", "crates/domain/src/recovery_eh_lookup.rs"),
)


def main():
    sources = sorted(RUST_ROOT.glob("crates/*/src/**/*.rs"))
    sources += sorted((RUST_ROOT / "src").rglob("*.rs"))
    output = RUST_ROOT / "target/llvm-cov"
    output.mkdir(parents=True, exist_ok=True)
    for package, module, relative_path in OWNERS:
        owned = RUST_ROOT / relative_path
        excluded = "|".join(re.escape(str(path)) + "$" for path in sources if path != owned)
        name = module.replace("::", "-")
        artifact = output / f"p1-{name}.lcov"
        subprocess.run(["cargo", "llvm-cov", "clean", "--workspace"], cwd=RUST_ROOT, check=True)
        subprocess.run([
            "cargo", "llvm-cov", "--package", package, "--lib",
            "--ignore-filename-regex", f"({excluded})",
            "--lcov", "--output-path", str(artifact),
            "--fail-under-lines", "95", "--fail-under-functions", "95",
            "--", f"{module}::tests::",
        ], cwd=RUST_ROOT, check=True)
        records = artifact.read_text().split("end_of_record")
        measured = [record for record in records if "SF:" in record]
        if len(measured) != 1 or f"SF:{owned}" not in measured[0]:
            raise SystemExit(f"{module}: unexpected production coverage denominator")
        totals = dict(re.findall(r"^(LF|LH|FNF|FNH):(\d+)$", measured[0], re.MULTILINE))
        print(f"{module}: lines {totals['LH']}/{totals['LF']}, functions {totals['FNH']}/{totals['FNF']}", flush=True)


if __name__ == "__main__":
    main()
