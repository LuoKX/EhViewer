# Full Rust application progress

Operator requirement: entire project-owned application in Rust, original UI and
interactions preserved, one completed feature per commit, parallel isolated work
approved. Existing app/data/user edits remain protected. This is not permission
for production cutover, network credentials, data migration on live files or
publishing/installing APKs.

## Accepted foundation (2026-10-07)

`native/` is a separate diagnostic application, not the original product UI.
Implemented: GameActivity/real AccessKit adapter binding, standalone build and
package checks for ARM64/x86_64/ARMv7; direct Rust navigation parser; original login
interaction reducer, measured layout contract and original-vector renderer;
source-bound resources and dependency provenance; one-owner raw Android input
hook and full UTF-16 transport; diagnostic one-shot runtime, local callback lease
checks, error/exit cleanup and independent native-buffer reset ownership.

Latest foundation gate: 55 native Rust tests; Python suites scripts42/parity30/
fork6; pure IME session100% line/function coverage; host/ARM64 Android all-target
Clippy clean; three-ABI debug APK and v1/v2 signature checks pass. Independent
review165c3850-149a-4dfd-8c44-23304459593d found no issues for bounded diagnostic
prototype only. Logs under ignored `native/build/logs/ime-*`. Source/fork archive
byte audit covers217 files, only two upstream sources changed.

**No device gates run.** Build/host/source tests do not prove original UI, IME,
TalkBack, secure text, autofill, typography, lifecycle, GPU or performance parity.

## Feature board

| Feature | Code / host evidence | Product / device gate | Commit policy |
| --- | --- | --- | --- |
| Existing Rust core and seven category rules | Already production-wired in original app; prior audit records retained | Original Android effect/device gates remain | Existing commits retained |
| Isolated Rust foundation | Accepted bounded diagnostic slice, gates above | Not production screen or cutover | Checkpoint existing work before new slices |
| DataStore preference format and Rust file runtime | Next bounded F1 seam, copied synthetic fixtures only | No live preference access; settings UI/upgrade still pending | Commit after compatibility, failure and independent review gates |
| Original secure-text behavior | Next source-bound seam, pinned Compose oracle | Actual IME/edit/render/autofill still pending | No stock-password substitution or product completion claim |
| SQLite data compatibility | Inventory historical schemas and copied fixture plan | No live DB or destructive migrations | Own feature commits only after implemented gates |
| Full F1–F8 application responsibilities | Pending; reuse existing Rust owners | All original behavior/data/device/cutover obligations open | One finished bounded feature per commit |

Parent serializes integration and commits. Workers own disjoint isolated
snapshots; no shared root writes, commits, installs, credential use or live data.
Reports and gate logs remain outside tracked source. Exact completed commits are
reported through Git history; pending sub-features are not renamed completed.

References: [full responsibility map](full-rust.md), [original UI contract](ui-parity.md),
[ADR0008](adr/0008-full-rust-application.md), [native checks](../../native/README.md).
