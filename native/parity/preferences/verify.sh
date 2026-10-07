#!/usr/bin/env bash
# Real pinned-dependency harness; no std-only external-crate pretense.
set -euo pipefail
cd "$(dirname "$0")/../../.."
exec python3 -B native/parity/preference-lock/verify.py
