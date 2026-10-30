# Full Rust application progress

## Current direction (2026-10-29)

Full Rust application development is **paused at the operator's request**, at
`253852b49`. Retain the accepted `native/` foundation and isolated pending work;
do not continue F0–F8 implementation or the queued favorite-policy integration.

The operator subsequently approved implementing the five bounded follow-up
migrations in the existing Android application, reusing `app/src/main/rust/`:
recovery mapping, download file rules, update packages, crop policy and comment
preprocessing. The original Android UI, platform services and storage remain in
place. This approval does not resume `native/` or its queued favorite-policy work.
Those five slices are implemented and host/ABI validated. After the follow-on
audit, the operator approved the next three groups: startup state projection,
search/pagination state, and label/entry-action planning. Implementation now uses
four additional domain owners (`download_startup`, `catalog_state`, `label_rules`,
`entry_actions`) and five JNI V1 endpoints in the original app. Validation passed:
607 Rust tests, 442 JVM tests per Android flavor, 100%
line/function coverage for each new owner, and four verified ARM64/x86_64 APKs.
Device test sources compile; no device was connected. Full evidence is recorded
in the migration audit. Recovery merge, reader interaction and thumbnail flow
remain later candidates.

After that review, the operator approved its first three groups. The third
batch now connects favorite date/last-page navigation, quick-search creation and
batched duplicate checks, shared list positions, label editor/settings menus,
and bulk favorites/local-favorite overlays. It adds `quick_search`, `list_order`
and `favorite_batch` owners while reusing the existing catalog/library/label
contracts. Validation passed: 620 Rust tests, 458 JVM tests per flavor,
100% production
line/function coverage for each new owner, and four verified ARM64/x86_64 APKs
with exact 130/129 exports. Device tests compile; no device was connected.
Recovery transaction merges, reader interactions and thumbnail flow remain later
work; `native/` development stays paused.

The operator then approved the fourth audit's first three groups. The fourth
batch now implements recovery formal merges/staging/finalization, reader
source/mode/discrete navigation, and thumbnail phases through three new domain
owners and three JNI V1 endpoints. Room retains the transaction and rechecks;
Android retains loader/animation/cache/filesystem effects. Reader navigation
objects retain their effective-mode identity, including equivalent default and
explicit preferences. Five old region definitions and their unused inversion
helper are replaced by the Rust region policy.

Validation passed: 634 Rust tests, 474 JVM tests per flavor, 100% independent
line/function coverage for all three new owners, 14 host JNI fixtures and four
verified ARM64/x86_64 APKs with exact 133/132 exports. Strict Clippy/rustdoc,
scoped Spotless and CI script checks passed; Android lint reported zero errors
and the same two existing warnings. Both device-test variants
compile; no device was connected, so actual Room rollback and UI/SAF runtime
cases remain unexecuted. A stale core/data DEX cache referenced the old workspace;
cleaning that module's generated output and rebuilding without the build cache
resolved it. All 1,781 frozen source/config hashes were unchanged during APK verification;
the 414 native files and prior three batches are preserved.

Next audit candidates are detail-preview integer paging, comment/tag/rating UI
policies, and settings/display rules. These are not part of the fourth batch;
full Rust application development stays paused.
See the 2026-10-29 audit and implementation record in the
[migration boundary audit](../audits/rust-migration-2026-10-06.md).

Original full-application target, currently paused: entire project-owned
application in Rust, original UI and interactions preserved, one completed
feature per commit, parallel isolated work
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
| DataStore preference format and Rust file runtime | Accepted valid-UTF8 codec + synthetic host store: 38 actual JVM cases, 25 focused Rust tests, 9 checker tests; owner lines98.64%/functions98.08% | Android storage, malformed-UTF8 equivalence, settings defaults/UI/upgrade and device durability pending; no live paths | Committed `1931379ed`; no product completion claim |
| Original secure-text behavior | Accepted source-bound policy: 12 Rust tests, 17 whole-class pinned proofs, 24 drift/checker tests; owner lines98.50%/functions100% | Actual IME/edit/render/autofill and timer delivery still pending; helper is not password widget | Committed `af7c40ea9`; no product completion claim |
| Pinned JVM UTF8 replacement helper | Accepted standalone host foundation: 258,862 generated PreferenceMap value/set cases, 5 Rust tests, 14 proof/runtime tests; owner lines98.39%/functions100% | Original preference codec unchanged; Android libcore/device proof and production consumer integration open | Committed `f665e62eb`; no product completion claim |
| SQLite data compatibility | Accepted original23-schema/FTS host foundation: 5 Rust tests, 40 Python negatives, actual SQLite differential; owner lines/functions100%; independent parent recheck passed | No persistent engine, migrations, live DB, Room/device or destructive operations | Committed `1d7125e93`; no engine/product completion claim |
| Safe private preference descriptors and locks | Accepted exact rustix1.1.5 Linux/Android descriptor seam, stable crash-released lock inode, no-follow/FIFO rejection and anchored atomic fixture updates;125 aggregate Rust; runtime100%/lock99.20% line and100% function gates | Actual Android locks/storage, power loss, retained bare-fork descriptors, MSRV1.95 and full F1/settings/UI remain open | Own bounded filesystem feature commit; no product cutover |
| Favorite name/count snapshot policy | Reviewed isolated ten-key/default/read-only Number.intValue projections and owned exact-ten atomic plans;356 JVM records/9 favorite tests;69/69 lines14/14 functions; declaration excerpt P2 closed | Current-root reconciliation/replay/integration pending; per-index concurrency, persistence/flows/UI/Android/device/full F1 not accepted | Not committed; retain frozen original source and disclosed failures |
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

JVM UTF8 integration gate (2026-10-07): original decoder/Oracle/golden/JDK/cache
identities unchanged, pinned absolute JVM launchers retained. Parent separately
preserves exact archived92 isolation inventory and live-checks77 original parser/
bytecode/fixture inputs; no rollback or relabeling of accepted preference proofs.
Rejected output directories/symlink/traversal cannot receive failure ledgers;
JSON errors remain payload-free. Parent integration review
c301a6ea-fcdd-4d1b-a1e2-76e9b71b3599 accepts standalone host foundation with no
P0/P1. Its clean-checkout oracle prerequisite P2 is fixed in native/README.md.
Parent reran 102 aggregate native Rust tests, strict all-target Clippy, exact
258,862-case JVM replay/owner coverage,14 proof tests, unchanged38-case preference
replay and three-ABI APK/v1-v2 validation. Logs: ignored
`native/build/logs/jvm-utf8-parent-*`. Original value/key decoding and all UI/
Android behavior remain unswitched; host JDK proof is not Android libcore proof.

Schema parent gate (2026-10-07): exact original Rust module/generated descriptors/
tests/SQL/source inventory/frozen fixtures copied; restored verifier retains
original SHA `af2ef2c6…`. Library registration and owned checker/docs manifests
explicitly rebound, not source/golden identity. Parent normalizes parser failures
and executes user_version from retained constant statement table after existing
integer/namespace validation; one direct negative added. Actual23-version compiled
Rust-to-SQLite differential,5Rust/40Python,100% owner gate,107aggregateRust,
strict all-target Clippy and three-ABI APK/v1-v2 pass. Earlier preferences38/
JVM UTF8258,862 replays remain exact. Logs: ignored
`native/build/logs/schema-parent-*`. Independent current-root integration review
ee53fc4a-2998-412d-889d-83b596a12fb6 found no issues and accepts host foundation
only. Missing Eh1–3, migrations/persistent engine and Android/Room/UI gates open.

Filesystem parent gate (2026-10-07): private proof packaging/binding P1s and
production-only coverage P2 closed by b0653c8c-34ca-4885-b4b5-ba6b3b20926c.
Current parent integrates exact reviewed owner/tests/Cargo bytes, preserving
current UTF8/schema modules and original codec byte region/JVM goldens. Exactly
one existing rustix1.1.5 target dependency/root edge; no package/version/checksum
or other-edge changes. Native-library registration and selected owned input
hashes reconciled; secure-text classes/cases and schema data unchanged. Three JSON
parse sites now normalize payload-free failures; direct negative raises checker
suite to12. Original codec checker9 retained. Source-only Python-B replay depends
on no generated cache; actual source/test mismatches and omitted bindings reject.

Parent125 aggregate Rust tests, focused52-command replay, repeated kernel/process
stress, exact preferences/JVM UTF8/23-schema regressions and all-target strict
Clippy completion passed. Dead-code warnings retained, not warning-free claim.
Runtime51/51lines5/5functions; production lock124/125lines14/14functions. Review
af0de463-887a-4177-95a2-b5b0f2733888 accepts source/host integration with notes.

ABI evidence correction: initial `--jobs 2` builder invocation was arm64-only;
its claimed three-ABI APK result explicitly withdrawn. Fresh command repeats
`--abi arm64-v8a --abi armeabi-v7a --abi x86_64 --jobs 2`, passes actual build,
GameActivity JNI staging, APK library/ELF/frame-class/signature/alignment/identity
checks. Exact argv/result/source/library/APK identity packet retained under
ignored `native/build/logs/preference-lock-parent-three-abi-proof.json`; old log
preserved. Targeted reviewer89e0e69d-e59c-495c-877a-463e5cdfd387 closes this P2;
new evidence is not retroactive pass. No install/device/runtime/power-loss,
arbitrary bare-fork, MSRV1.95, global-cache isolation, product/UI/full F1 acceptance.

Queued integration (paused): original favorite snapshot policy. Real pinned
compiler/JVM Number.intValue discovery supersedes parent's early nonInteger rejection
assumption; stored numeric kind/bits/wire remain unchanged. Fresh isolated review
fee6a7e2-764d-4821-9029-bb016f41d79f accepts bounded policy with notes; parent
adds exact original declarations73-74 while retaining historical omission,
targeted recheckb9c68002-055c-4994-b09a-71ab7c85c11c closes excerpt P2. Initial
rustup/network attempt, imported-test output violation, timeout, moving-root hash
assertion and generic-stdout misplacement remain disclosed failures. Snapshot
proof never relabeled current-root/UI/storage/device completion.

Resource failures
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
