#!/usr/bin/env python3
"""Gate update policy/protocol as independent production owners at 95%."""
if __package__:
    from .check_p3_coverage import main
else:
    import sys
    from pathlib import Path

    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from scripts.check_p3_coverage import main

OWNERS = (
    ("eh_domain", "update_policy", "crates/domain/src/update_policy.rs"),
    ("eh_net", "updater", "crates/net/src/updater.rs"),
)

if __name__ == "__main__":
    main(OWNERS, "updater")
