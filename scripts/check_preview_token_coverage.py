#!/usr/bin/env python3
"""Gate preview token policy independently, excluding codec and test modules."""

if __package__:
    from .check_p3_coverage import main
else:
    from check_p3_coverage import main

OWNERS = (
    (
        "eh_domain",
        "spider_info::preview_tokens",
        "crates/domain/src/spider_info/preview_tokens.rs",
    ),
)

if __name__ == "__main__":
    main(OWNERS, "preview-tokens")
