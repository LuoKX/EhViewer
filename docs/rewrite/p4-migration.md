# P4 remaining portable policy migration

## Ordered owners and current status

1. BBCode export and edit classification/slice planning: implemented below.
2. ComicInfo refresh/export preparation policy: implemented below; XML codecs remain Rust.
3. Recovery scanning/cleanup rules: implemented below; Android/SAF effects stay Kotlin.

HTTP execution, Room/database replacement and remaining image codecs remain
separate high-risk work. This stage does not change networking, credentials,
database persistence, image decoding or filesystem execution.

## BBCode production owner

`eh_domain::bbcode` now owns UTF-16 tag emission, multi-style priority selection,
stable range ordering and the legacy tag stack. `bbcode::diff` owns the greedy Myers path, tie-breaking,
single-delta classification and original/new text slice coordinates.

`BBCodeTextContextMenu.kt` projects opaque Compose styles to observations and
applies Rust-planned slices. Compose retains actual annotation clipping, style
objects, paragraph/string annotations, selection, composition and toolbar effects.
There is no Kotlin diff, tag-stack traversal, shadow calculation, retry or
error-driven business fallback. Existing no-style fast path still avoids JNI.

Removed runtime dependency and version-catalog entry:
`io.github.petertrr:kotlin-multiplatform-diff:1.3.0`.

New bounded V1 direct-buffer/CBOR endpoint:
`BBCodeBridgeKt.nativePlanBBCodeV1`. Prior contracts are unchanged. Requests and
results use raw UTF-16 units, not lossy CBOR text, preserving lone surrogates,
emoji boundaries and NUL. Android validates result action, ranges, unit values,
length arithmetic and unchanged prefix/suffix before applying a splice.

Unlike the P3 normalization-only contract, this operation needs comment text in
Rust memory. No text is logged or persisted. Compose objects never cross JNI.

## Compatibility and explicit policy changes

The frozen Kotlin dependency generated 2,704 differential cases, including
repetitions, tie-breaking, insertion/deletion positions, replacements,
multi-hunk changes, CR/LF, CJK, emoji and malformed UTF-16. Every legacy single
INSERT/DELETE coordinate is exact. Unchanged text reuses the original annotated
string; complex edits retain the incoming value, matching the old multi-delta
behavior.

Single CHANGE deltas formerly reached Kotlin `unreachable()` and crashed.
They now retain incoming text without reusing original styles. Incoming selection
and composition remain unchanged. This is an intentional bug fix, not claimed
legacy error parity.

Export preserves bold/italic/underline/strikethrough priority, same-start
longest-end-first ordering and stable equal-bound order. Crossing spans and empty
or unsupported stack observations retain legacy behavior, including historically
unbalanced tags. Normalization still removes empty/unsupported style observations
before the normal send path. Improving crossing-tag semantics is separate work.

Budgets:

- 65,536 UTF-16 units per source/target; 100,000 export observations.
- 1,000,000 exported UTF-16 units; 8 MiB encoded JNI request, checked before decode.
- 1,000,000 diff work steps and 100,000 path nodes. On diff budget exhaustion Rust
  returns TARGET atomically, retaining incoming text rather than partial slices.
  Large edits can therefore lose original styles under this explicit policy.
- Invalid input, transport/linkage failures and typed Rust errors stay observable;
  there is no catch-and-recompute Kotlin fallback. Oversized export/editor inputs
  fail validation instead of entering unbounded diff allocation.

Myers code retains Apache-2.0 notices/license and pinned oracle provenance in
`crates/domain/src/bbcode/test_data/README.md`.

## BBCode stage acceptance evidence

Local acceptance at the BBCode stage:

- 492 Rust tests/doctests and 267 Kotlin JVM tests pass.
- Shared exact request/response CBOR goldens pass for export (including multi-style
  priority) and edit.
- Host and ARM64 Clippy, Rustfmt, Rustdoc, rewrite naming and coverage-mask tests
  pass; `:app:spotlessCheck` passes. One existing WebP device-test file received
  formatter-only cleanup; no decoder behavior changed.
- Rust LSP reports no diagnostics. Default `kotlin-lsp` is unavailable; Kotlin
  uses Gradle compilation/tests. Install it or update `pi-lsp.json` for LSP checks.
- Default and Marshmallow Kotlin compile; `P4MigrationDeviceTest` instrumentation sources compile. Tests cover real-JNI
  export/normalization, insertion/deletion annotations, unchanged text,
  selection/composition, replacement and multi-hunk editing.
- `BBCodeJniSmoke.java` executes real host JNI: exact frozen responses for both
  operations, immutable request buffers, size/direct-buffer rejection and atomic
  failure on too-small output. This does not prove Android UI execution.
- Host V1 symbols match 79/79. ARM64 Release library matches all 102 Android JNI
  exports and retains `JNI_OnLoad`; APK contains only arm64-v8a.
- Release runtime dependency report no longer contains the retired diff library.

Independent `scripts/check_p4_coverage.py` gates exclude every other production
owner and test source; no cross-owner/test coverage borrowing:

| Owner | Lines | Functions |
| --- | --- | --- |
| BBCode serialization/policy | 105/106 | 11/11 |
| Myers edit planning | 169/172 | 10/10 |

All eight P3 and both P4 independent gates pass locally. CI runs both P4 gates
and real host JNI smoke; older per-slice masks exclude BBCode
owner/tests. Retired-source check rejects old diff imports in production Kotlin.

Android device/UI execution and ARM64 latency/peak-memory measurements remain
UNRUN. Native symbol checks, host JNI and APK/instrumentation builds are not
claimed as device runtime or performance acceptance.

## ComicInfo refresh and archive preparation owner

`eh_domain::comic_info_policy` now owns preparation validation, metadata refresh
eligibility/failure mode, positive export-page selection and keep/create/rewrite
selection. New fixed-size V1 endpoint: `ComicInfoPolicyBridgeKt.nativePlanComicInfoPolicyV1`.

Only operation, directory/detail/refresh observations, page counts and XML-read
status enter JNI. XML content, paths, gallery titles, tokens and credentials do
not. Existing Rust XML projection/codecs and archive writer contracts are unchanged.
Encoded policy requests are capped at 4 KiB before native decode; work/allocation
are constant. Counts remain signed Java Int observations, including zero/negative
existing counts; preparation/export results must be positive.

`SpiderDen` applies plans through `ComicInfoPolicyExecutor.kt`. Android retains
network calls, nullable XML read observations, whole-model copy, SAF/filesystem
writes, gallery mutation and Room/DownloadArtist effects. There is no local
page fallback/refresh decision, policy retry or Kotlin business fallback.

Compatibility:

- Preparation validates the pinned expected count before any metadata read,
  network, image descriptor acquisition or write. Same positive-count error and
  required-directory `IllegalArgumentException` messages are retained.
- Unread/missing/present observations are distinct. Rust requests at most one XML
  observation; Android makes one initial/final plan pair, not a retry loop.
- Known positive gallery pages skip XML entirely. Otherwise a positive decoded
  metadata count is required; zero/negative/missing counts retain the old error.
- Missing metadata requests refresh only for non-detailed galleries. Ordinary
  exceptions are logged and creation continues; cancellation and non-Exception
  failures propagate. Expected downloaded pages are applied after refresh, so
  server-updated counts cannot overwrite the pinned operation count.
- Existing XML with wrong pages is copied with only PageCount changed; no fresh
  projection, network or artist-index update. Matching XML is not rewritten.
- Direct writes skip directory-less effects and refresh only when requested for
  non-detailed galleries. Refresh errors propagate. No pre-fetch page snapshot
  is reapplied; newly fetched metadata/pages feed the existing Rust XML builder.
- Fresh XML write precedes artist persistence. Write failure stops artist update;
  stale-count correction does not touch artist rows. Existing decode-to-null
  semantics are retained, but the observer adds no catch-and-reread fallback.

Current local acceptance after this stage:

- 498 Rust tests/doctests and 284 Kotlin JVM tests pass. Added scalar branch-matrix,
  exact CBOR, observable error, lazy-read, bounded-phase, cancellation, ordinary
  network failure, direct-refresh mutation and file/artist effect-order tests.
- 13 real host JNI responses match frozen Rust bytes, including legacy domain
  errors. Request immutability, direct-buffer/size rejection and atomic short-output
  failure pass. Prior BBCode host JNI smoke remains green.
- Three P4 and eight P3 independent gates pass. New owner: 96/96 lines, 9/9
  functions; every other production owner/test source is excluded.
- Rustfmt, host/ARM64 Clippy, Rustdoc, naming/mask checks and app Spotless pass.
  Rust LSP reports no diagnostics; unavailable Kotlin LSP remains covered by
  Gradle compilation/tests rather than claimed LSP acceptance.
- Default/Marshmallow Kotlin and instrumentation sources compile; ARM64 Release
  APK builds. Host V1 exports match 80/80; all 103 Android Java JNI exports match,
  JNI_OnLoad is retained, and APK ABIs remain arm64-v8a only.
- `ComicInfoPolicyDeviceTest` covers actual scalar JNI. Extended
  `SpiderDenArchiveDeviceTest` covers stored-field preservation, matching-XML
  no-write behavior, malformed metadata recreation and post-refresh pinned pages.
  These device sources were compiled, not executed.

CI adds policy host JNI smoke and independent owner coverage; all older slice
masks exclude the new owner/test subtree. Prior BBCode worktree changes remain
intact. No Git commit is made by this stage.

Real Android/SAF/database execution, network integration and ARM64 latency/peak
memory remain UNRUN. Recovery scanning/cleanup implementation follows below.

## Recovery discovery and stable cleanup production owners

`eh_domain::recovery_scan` owns root-inclusive BFS queueing, raw UTF-16 provider
ID deduplication, depth decisions, discovered-node and elapsed-time budgets.
`eh_ffi::recovery_scan` owns bounded, monotonically allocated handles. Kotlin
`RecoveryScanSession` is a scoped `Closeable` transport; `SafTreeWalker` retains
platform enumeration, clocks, coroutine cancellation, node materialization and
progress callbacks. No Kotlin BFS queue, seen-set or fallback policy remains.

`eh_domain::storage_rules` owns physical child UTF-16 ordering, directory/archive
candidate classification, directory-first identity ties, candidate sorting/capping
and stable newest-first cleanup indices. `DownloadRecoveryCoordinator` observes
candidate facts in 1,024-node chunks; `FileUtils.cleanupDirectory` validates the
whole index plan before retaining its old non-recursive `File.delete()` effects.
Large candidate identities do not trigger the generic four-times-request output
allocation: response buffers are bounded by output slot counts instead.

Compatibility retained:

- Root counts as one node even for zero/negative scan limits. An empty BFS queue
  does not spuriously exceed the deadline/node limit; equality stops only when
  more work remains. Deadline addition retains Java signed-long wrapping.
- Physical symlinks are skipped. Provider order stays unchanged; repeated root
  IDs, cycles and depth-rejected IDs remain deduplicated. Excess directory depth
  sets the limit flag and continues siblings; loose files remain observable one
  level beyond the directory depth cap.
- Java String UTF-16 ordering, lone surrogates, directory-first candidate ties,
  stable timestamp ties and zero/negative retention counts remain intact.
  Negative candidate counts retain the exact Kotlin `take` exception message.
- Existing page-name Regex is eight ASCII digits plus the image-extension
  whitelist. Its device-specific Unicode case-folding result remains an Android
  observation; it is not silently reinterpreted by host Java or Rust.
- Cancellation, provider/progress failures and native errors propagate; scoped
  close releases native state. Close also releases entries after mutex poisoning.
  Closed/stale handles cannot be used or recycled. No retry/Kotlin fallback.
- Incomplete discovery/candidate observations retain `limitExceeded`, so the
  existing coordinator does not mark unobserved sources missing.

New hard bounds constrain new/native working sets: at most four live scans,
100,000 nodes/provider IDs, 128 KiB per provider identity and 8 MiB aggregate
retained identity bytes per scan. Sorting/candidate observations allow 100,000
items and 16 MiB aggregate raw names, with 32 MiB maximum CBOR input. Overlarge
physical enumerations/provider identity sets stop with the limit flag; overlarge
candidate identities retain a bounded prefix and report incomplete selection.
Invalid cleanup plans fail before any deletion. These are safety ceilings, not
claims that Android provider cursors or `File.listFiles()` allocations are bounded.

Latest local acceptance:

- 517 Rust tests/doctests and 299 Kotlin JVM tests pass. Added 1,024 clocked
  physical/provider reference graphs and 1,536 frozen JDK name, retention and
  candidate-order cases. Reference generator reproduces the frozen bytes.
- Six P4 and eight P3 independent production coverage gates pass. New owners:
  domain scan 140/140 lines, 10/10 functions; storage 141/141, 19/19;
  handle registry 80/80, 9/9. Other production/test files cannot contribute.
- Real host JNI returns four exact storage golden envelopes. Scalar scan probes
  cover BFS/depth/quota, raw-surrogate deduplication, phase errors, capacity,
  stale handles and 500 scoped closes. Prior BBCode/ComicInfo smoke remains green.
- Host V1 exports match 84/84; all 107 Android Java JNI exports plus JNI_OnLoad
  match the manifests. Release APK contains arm64-v8a only.
- Rustfmt, workspace/all-target and ARM64 Clippy, Rustdoc, naming/mask checks,
  Spotless, Default/Marshmallow Kotlin and instrumentation-source compilation pass.
  Rust LSP reports no diagnostics; `kotlin-lsp` remains unavailable, not accepted.
- New `RecoveryScanDeviceTest` covers real-JNI physical BFS/depth/symlinks,
  cancellation handle release, cleanup effects and UTF-16 plans. Sources compiled,
  not executed. Real SAF provider/UI behavior, ARM64 latency and peak memory UNRUN.

CI adds both host JNI probes and independent coverage owners; older slice masks
exclude both new owner/test trees. Shared fixtures are test-only, never runtime
assets. Prior BBCode/ComicInfo worktree changes remain intact; no Git commit.

Remaining Android adapter work includes directory sidecar selection/fingerprint
observation, archive inspection effects/budgets and promotion path/Room mapping.
Do not treat this stage as an HTTP, image codec or database replacement. Real
P3/P4 device and memory acceptance remains required before those higher-risk moves.
