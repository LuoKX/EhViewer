#!/usr/bin/env python3
"""Gate each P2 production owner independently; no cross-owner/test coverage borrowing."""
import re
import subprocess
from pathlib import Path

RUST_ROOT = Path(__file__).resolve().parents[1] / "app/src/main/rust"
OWNERS = (
    ("eh_domain", "reader_policy", "crates/domain/src/reader_policy.rs"),
    ("eh_domain", "transfer_speed", "crates/domain/src/transfer_speed.rs"),
    ("eh_domain", "recovery_search", "crates/domain/src/recovery_search.rs"),
    ("eh_domain", "spider_info", "crates/domain/src/spider_info.rs"),
    ("eh_domain", "local_pages", "crates/domain/src/local_pages.rs"),
    ("eh_net", "cache_keys", "crates/net/src/cache_keys.rs"),
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
        artifact = output / f"p2-{name}.lcov"
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
