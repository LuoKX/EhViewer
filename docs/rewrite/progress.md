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
| Isolated Rust foundation | Accepted bounded diagnostic slice, gates above | Not production screen or cutover | Committed `858529fe3` |
| DataStore preference format and Rust file runtime | Accepted valid-UTF8 codec + synthetic host store: 38 actual JVM cases, 25 focused Rust tests, 9 checker tests; owner lines98.64%/functions98.08% | Android storage, malformed-UTF8 equivalence, settings defaults/UI/upgrade and device durability pending; no live paths | Separate bounded primitive commit after independent rechecks |
| Original secure-text behavior | Accepted source-bound policy: 12 Rust tests, 17 whole-class pinned proofs, 24 drift/checker tests; owner lines98.50%/functions100% | Actual IME/edit/render/autofill and timer delivery still pending; helper is not password widget | Committed `af7c40ea9`; no product completion claim |
| SQLite data compatibility | Inventory historical schemas and copied fixture plan | No live DB or destructive migrations | Own feature commits only after implemented gates |
| Full F1–F8 application responsibilities | Pending; reuse existing Rust owners | All original behavior/data/device/cutover obligations open | One finished bounded feature per commit |

Secure-text integration gate (2026-10-07): prior P1 proof gaps and P2 toolbar
wording resolved; independent re-review6eedbd1b-32c0-41cf-9fa1-06f6db0f88f0
accepts source/policy seam only. Parent reran 67 native Rust tests, strict host
Clippy, source/checker/owner gates and three-ABI APK/signature validation. APK
not installed. Complete class proofs preserve original 13 class identities and
add four helpers/mode classes from the same pinned artifact; no Cargo/source
version substitution. Logs: ignored `native/build/logs/secure-final-*`.

Preferences integration gate (2026-10-07): duplicate effective-unset P1 fixed
and independently rechecked31faf9f9-4194-4d13-9dcd-e6c1dc4a0ca0; parent
registration/checker/evidence delta approvedfd13ad1d-f97e-4200-ae4a-0286f8b34329,
final formatted-zero checker fix approved536a57d7-1591-49d4-817c-500ec4359925.
Parent reran 97 aggregate native Rust tests, strict host Clippy, Python suites
42/30/6/24/9, exact JVM replay/owner coverage and three-ABI APK/signature checks.
Root application never calls fixture storage. Logs: ignored
`native/build/logs/preferences-final-*`. These checks do not accept Android
persistence, full F1, minimum-toolchain execution or original UI/device behavior.

Next isolated components: safe Rustix Android filesystem ownership, exact JVM
UTF8 replacement helper and original23-schema/FTS contracts. Resource failures
are not accepted work: temporary storage was exhausted, two workers paused or
failed before validation. Parent preserved partial source and moved only owned
generated caches to private disk storage; retained native runs recover and rerun
all gates before review/integration. No execution-mode fallback or user-data use.

Parent serializes integration and commits. Workers own disjoint isolated
snapshots; no shared root writes, commits, installs, credential use or live data.
Reports and gate logs remain outside tracked source. Exact completed commits are
reported through Git history; pending sub-features are not renamed completed.

References: [full responsibility map](full-rust.md), [original UI contract](ui-parity.md),
[ADR0008](adr/0008-full-rust-application.md), [native checks](../../native/README.md).
