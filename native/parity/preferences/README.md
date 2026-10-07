# Preferences F1: validated subset, synthetic host runtime

## Scope / approval gates

Implemented codec + synchronous synthetic fixture runtime. **Not full settings migration, complete Jetpack corrupt-input equivalence, Android persistent execution, UI cutover, coroutine flow, or device durability.** No JVM dependency in APK. No production data/paths/credentials/network accessed. Cargo manifests/lockfiles and original sources unchanged. Parent registers the standalone module in `lib.rs`; diagnostic application does not call storage or discover application paths.

Supervisor-approved narrowing:
- JVM 1.2.1 accepts malformed UTF8 value/set strings using replacement characters; Rust rejects with `InvalidUtf8`. Keys reject in both. Original-compatible corrupt-value decoding remains **OPEN F1 gate**. No `from_utf8_lossy` approximation.
- Installed Rust std excludes Android from supported kernel file-lock cfg. `FixtureStore::open` on Android returns `LockUnsupported` before filesystem writes. **Android persistent runtime blocked**, pending separately approved safe locking seam. Android metadata compilation is not device execution.
- Rename commits bytes. Post-rename directory-sync failure returns `Ok(DirectorySyncUnavailable)`, with in-memory baseline already advanced. Precommit errors do not overwrite target; noncooperating external changes remain external changes, not runtime writes.

## Audited original sources

Immutable snapshot 858529fe3:
- `gradle/libs.versions.toml`: datastore version 1.2.1.
- `Settings.kt`: `DataStorePreferences(null)`; bool/int/long/string/string-set delegates, nullable absence, separate array element keys, retired-flag removal and existing repair bridge.
- `DataStorePreferences.android.kt`: package-derived name, AndroidX factory, SharedPreferencesMigration, context.preferencesDataStoreFile.
- Common `DataStorePreferences.kt`, `PrefDelegate.kt`, `Snapshot.kt`, `Edit.kt`: explicit delegate defaults, null removes key, typed snapshots, DataStore edit, coroutine scheduling. Snapshot exposes named-key lookup/contains only, not map-key iteration, so canonical map key encoding has no observed consumer dependence in this storage API.
- `ArchivePageLoader.kt:55`: `archivePasswds.none(archiveReader::providePassword)` exposes set encounter order in password attempts. Sorted membership was insufficient: supervisor-approved `PreferenceSet` now keeps first-occurrence order for decode, merge, insertion, and encode. No consumer/core changes.

These policies are **not duplicated** in Rust codec. Missing file means empty map; caller/domain supplies defaults. No path discovery, SharedPreferences import, retired-flag cleanup, or settings repair implemented.

## Exact cached JVM evidence

`artifact-sha256.txt` binds generated proto jar, shaded external protobuf jar, and preferences-core Android AAR, all 1.2.1. `javap.txt` freezes generated constants/bytecode; `javap-map-entry.txt` external parser; `javap-serializer.txt` actual serializer, including `toSet` and VALUE_NOT_SET corruption. `Oracle.java` compiles against cached proto + external jars only. Synthetic `goldens/*.input.bin`, `*.jvm.bin`, `results.tsv` record actual parsing/re-encoding; `evidence-sha256.txt` binds oracle code and goldens. No inferred protocol fields.

| Message | Field | Number | Wire tag |
| --- | --- | ---: | ---: |
| PreferenceMap | preferences map entry | 1 | 10 |
| Map entry | key string / Value message | 1 / 2 | 10 / 18 |
| Value oneof | boolean | 1 | 8 |
| Value oneof | float fixed32 | 2 | 21 |
| Value oneof | signed int32 varint, not zigzag | 3 | 24 |
| Value oneof | signed int64 varint, not zigzag | 4 | 32 |
| Value oneof | string | 5 | 42 |
| Value oneof | StringSet message | 6 | 50 |
| Value oneof | double fixed64 | 7 | 57 |
| Value oneof | bytes | 8 | 66 |
| StringSet | repeated strings | 1 | 10 |

Oracle confirms negative int32 uses sign-extended ten-byte varint; float/double NaN payloads/signed zero preserved. Duplicate map keys last-wins; duplicate key members last-wins; scalar oneof last-wins. Repeated StringSet oneof fields inside Value merge; intervening scalar resets set. **Duplicate Value members inside map entry replace**, unlike repeated StringSet inside one Value. Missing key becomes empty string. Missing/unrecognized-only Value is retained privately during physical map reduction; only an **effective** unset value remaining after last-key-wins replacement is rejected by serializer/Rust, never defaulted. P1 goldens cover unset→valid, valid→unset, future-only→valid, unknown-entry unset→valid, repeated empty Value/entry members, and an unset different key. Earlier overwritten unknown fields still make the entire decoded document read-only. Boolean nonzero and int32 low-bit behavior match tested oracle.

Unknown fields at map/value/set/group levels remain exact original wire in Rust. JVM drops map-entry unknown fields; Rust retains untouched original. Any observed unknown field (even overwritten duplicate entry or wrong wire type for known field) prohibits entire-document mutation. No silent future-field loss. Supported untouched documents encode byte-for-byte; mutated known-only documents use canonical map key order and preserved string-set encounter order. Private ordered `PreferenceSet` deduplicates first occurrences exactly as serializer `toSet` does; no public mutable Vec/String access bypasses counts/byte bounds. Frozen JVM/Rust assertions `[雪,x,雪] -> [雪,x]`, merged `[a,b]`, replacement `[b]`, stable duplicate insertion, and explicit order encoding protect archive-password attempt fidelity.

## Bounds / invariants

Private map/original-wire/read-only invariants. Redacted Debug on PreferenceSet, Value, Preferences, FixtureEdit, FixtureStore; error categories contain no paths/keys/values/OS messages. Decoder bounds: 1,048,576 wire bytes; 4,096 map entries including duplicates; 4,096 repeated strings per active set including duplicates; 65,536 field tags; 32 unknown-group levels. Ten-byte u64 varints must not overflow; lengths/tags/wires/groups/UTF8 validated. Encoder applies same bounds; failed mutation preserves previous document. IEEE754 bits represented as u32/u64; opaque byte arrays never coerced.

## Host runtime contract

Caller supplies existing trusted private synthetic directory. Fixed fixture filenames `preferences.pb` and `.preferences.lock`, no app-name/path lookup. Dedicated lock inode persists; never unlink/recreate it. `File::try_lock` nonblocking, exclusive, crash-released on tested Linux host. No cloned/inherited owner handle exposed. Private RAII ownership guard explicitly unlocks on normal drop and load/decode errors, then closes file; this avoids transient fork-inherited descriptor retention during unrelated concurrent process spawn. Abrupt death remains kernel-released. All writers must honor same lock; caller must prevent lock-inode/directory replacement and untrusted symlink/path manipulation. Byte comparisons reject observed noncooperating drift but cannot make noncooperating writers atomic.

Runtime non-cloneable. Updates receive restricted `FixtureEdit`, not replaceable `Preferences`: stale copied whole documents cannot be submitted. Update clones current owned baseline, validates edits, compares source bytes, creates same-directory mode-0600 temp with create_new, writes/syncs file, rechecks drift, renames, immediately updates baseline, then syncs directory. Bounded collision retries never adopt/delete stale temp files. Crash-left temp files may remain; no automatic sweeping. Stable lock file mode 0600. Missing and existing-empty data files differ for drift detection.

`std-lock-evidence.txt` binds actual installed rustc 1.97.0-nightly (7c3c88f42, 2026-05-14) primary std source hashes/API stabilization/cfg. Manifest `rust-version = 1.95` is MSRV, not installed pinned toolchain; **Rust 1.95 execution not validated**. No toolchain/dependency installed.

## Replay

From assigned snapshot root:

```sh
bash native/parity/preferences/verify.sh
```

Requires already installed javac/java/rustc/clippy-driver/rustfmt, cached jars, installed three Android std targets, installed sysroot llvm tools, python3. Replay outputs under `native/parity/preferences/build`; checker test fixtures under ignored `native/build/preferences-checker`. No global cache/SDK/registry writes. `verify.sh` verifies hashes, recompiles/replays JVM goldens, checks Rust formatting and library/test missing_docs + unsafe deny + clippy all/pedantic deny, runs synthetic tests, verifies expected stale-snapshot compile failure, compiles Android metadata only, calculates owner-only line/function coverage. Private test hooks inject pre-write/file-sync/rename and post-commit directory-sync failures; real files are reopened to verify state. Hooks compile only under cfg(test).

Tests additionally cover actual directory-permission create failure, callback error/panic, corrupt/oversized/UTF8 source preservation, observed drift, absent-vs-empty, collision exhaustion, and **real helper-process ownership contention + forced termination + lock-inode-preserving reopen**. Fault injection and process kill are not filesystem power-loss proof. Parent/reviewer still owns full Cargo suite, module registration, three full Android ABI builds, Android lock/runtime solution and production migration/behavior gates.

Parent integration hardening: coverage checker catches missing/malformed evidence with payload-free diagnostics and refuses empty owner coverage. Nine synthetic Python checker tests cover JSON/shape/read/encoding/count failures, deduplication, decimal/scaled zero counts and threshold rejection; these synthetic percentages are not Rust coverage evidence. Original owner source/tests and JVM goldens remain unchanged. Evidence inventories deliberately rebind changed replay/checker/documentation files and add the new checker test. Native `lib.rs` input binding advances only for the reviewed `secure_text` and `preferences` registrations; all original Kotlin, Cargo, JVM and Rust-owner identities remain unchanged.

Do not integrate `build/` outputs. Committable artifacts: source/test files; this README; oracle; replay/coverage scripts; source/artifact/evidence hash manifests; frozen javap/std evidence; goldens; saved focused validation/coverage summaries; stale-snapshot compile-fail text fixture.
