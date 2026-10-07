#!/usr/bin/env python3
"""Enforce independent line/function coverage for a native source owner.

Consumes cargo llvm-cov JSON, not tests-run counts or whole-workspace averages.
This gate does not measure Android effect execution, visuals, branches, or devices.
"""

import argparse
import json
from pathlib import Path

OWNER = "native/src/login_state.rs"
OWNERS = (
    OWNER,
    "native/src/signin_layout.rs",
    "native/src/signin_artwork.rs",
    "native/src/ime_session.rs",
)
MINIMUM = 95


def check(report: dict, owner: str = OWNER) -> dict[str, float]:
    """Require one nonempty exact approved owner with both metrics at least 95%."""
    if owner not in OWNERS:
        raise ValueError(f"Unsupported native coverage owner: {owner}")
    files = [
        file
        for data in report.get("data", [])
        for file in data.get("files", [])
        if str(file.get("filename", "")).replace("\\", "/").removeprefix("./") == owner
        or str(file.get("filename", "")).replace("\\", "/").endswith("/" + owner)
    ]
    if len(files) != 1:
        raise ValueError(
            f"Coverage requires exactly one {owner} record; found {len(files)}"
        )
    percentages = {}
    for name in ("lines", "functions"):
        metric = files[0].get("summary", {}).get(name, {})
        count = metric.get("count")
        covered = metric.get("covered")
        if (
            type(count) is not int
            or type(covered) is not int
            or not 0 <= covered <= count
            or count == 0
        ):
            raise ValueError(f"{owner}: invalid or empty {name} coverage")
        percentage = covered * 100 / count
        if percentage < MINIMUM:
            raise ValueError(f"{owner}: {name} {percentage:.2f}% < {MINIMUM}%")
        percentages[name] = percentage
    return percentages


def main() -> None:
    """Check a recorded report; failures never become a success by omission."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--owner", choices=OWNERS, default=OWNER)
    args = parser.parse_args()
    try:
        percentages = check(
            json.loads(args.report.read_text(encoding="utf-8")), args.owner
        )
    except (OSError, ValueError, TypeError, AttributeError) as error:
        parser.exit(1, f"Native coverage gate failed: {error}\n")
    print(
        f"{args.owner}: lines={percentages['lines']:.2f}% functions={percentages['functions']:.2f}%; gate passed"
    )


if __name__ == "__main__":
    main()
