#!/usr/bin/env python3
"""Gate P4 portable rules as separate production owners."""
from check_p3_coverage import main

OWNERS = (
    ("eh_domain", "bbcode", "crates/domain/src/bbcode.rs"),
    ("eh_domain", "bbcode::diff", "crates/domain/src/bbcode/diff.rs"),
    ("eh_domain", "comic_info_policy", "crates/domain/src/comic_info_policy.rs"),
    ("eh_domain", "recovery_scan", "crates/domain/src/recovery_scan.rs"),
    ("eh_domain", "storage_rules", "crates/domain/src/storage_rules.rs"),
    ("eh_ffi", "recovery_scan", "crates/ffi/src/recovery_scan.rs"),
)

if __name__ == "__main__":
    main(OWNERS, "p4")
