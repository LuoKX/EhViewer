# ADR 0007: Single Strict Rust Core Selection

## Status

Accepted for the remaining Rust migration. Supersedes ADR 0004 rollout selection semantics.

## Context

The staged rewrite accumulated 32 independently persisted `useRustCore*` settings. Their production semantics are not uniform: some choose one parser, some compute legacy first and compare Rust, some catch JNI/linkage failures and return legacy, and some read preferences repeatedly during a long operation. The 21-setting MVP coordinator does not cover 11 production switches and cannot guarantee that an enabled Rust configuration actually uses Rust for every relevant operation.

Runtime parity and automatic fallback also weaken failure evidence. A Rust boundary can be broken while the application silently succeeds through legacy, so enabling a collection of flags does not establish Rust ownership.

## Decision

### Persisted state

Android persists exactly one user setting, `use_rust_core`, exposed as `Settings.useRustCore`. Its default is `false`.

The first migration from the 32 old keys is one atomic DataStore mutation:

- If the new key exists, preserve its value.
- Otherwise set it to `true` only when every old key is present and `true`.
- Any absent, false, or mixed old state becomes `false`.
- Remove all old keys in the same mutation.
- Repeating the migration is idempotent.

Old key names may remain only in the migration implementation and migration tests.

### Runtime selection

Each complete operation captures one backend before work begins. A long operation stores that snapshot in its operation context or native handle and never rereads the setting mid-operation.

| Backend | Contract |
| --- | --- |
| `Legacy` | Execute only the legacy business implementation and do not call the corresponding Rust business entry. |
| `Rust` | Execute only the Rust business implementation and do not compute, call, compare, replay, or fall back to legacy. |

JNI linkage, symbol, CBOR, envelope, contract, parser, domain, and resource-limit failures are surfaced through the existing typed error boundary. The only operational recovery is to disable the global setting and start a new operation.

Production runtime parity is removed. Compatibility remains protected by frozen legacy characterization tests and shared Kotlin/Rust good/base/bad golden fixtures. A mismatch fails tests; it never selects a production result.

### Ownership boundary

Rust owns portable business rules, parsers, request/query plans, validation, sorting/filtering and state machines. Kotlin/Android retains Compose, lifecycle, Room, DataStore, SQLite files, Paging, WorkManager, notifications, permissions, SAF, platform network transport, CookieManager and execution of platform transactions/effects.

Rust Core health and low-level Rust infrastructure without a legacy business alternative are not user-selectable modules. The global setting selects dual-path business operations; it does not disable independent diagnostics.

### Exposure and completion

The new setting defaults off and is not exposed as user-enablable until all mapped existing and remaining portable functions have strict Rust paths. The archive gate includes ZIP/CBZ, RAR/RAR5, 7z and tar, plus the encrypted-ZIP password flow. Encryption rejected by the pinned codec must surface as an explicit Rust failure and must not select the legacy reader.

C1-C6 implementation is complete; exposure and the final completion claim still require C7 non-device acceptance. Real-device execution is deferred to the parent rewrite task. Host-side success must not be described as device acceptance.

## Alternatives Considered

- Keep the 32 independent settings: rejected because mixed ownership and partial rollout remain valid production states.
- Keep the 21-setting coordinator: rejected because it excludes 11 active switches and persists no authoritative selection.
- Use `master || module`: rejected because it retains two sources of truth.
- Automatically disable or fall back after a Rust failure: rejected because it hides broken Rust ownership and may replay side effects.
- Run legacy and Rust in production for parity: rejected because it doubles work and can mutate or consume inputs twice.

## Consequences

- Operators lose per-module rollback; rollback is the global setting only.
- Rust failures become visible and may fail an operation instead of succeeding through legacy.
- Legacy implementations remain reachable only from global-off mode until final removal is separately approved.
- Tests and CI must prove mutual exclusion rather than successful fallback.
- Every migration slice must use the operation snapshot and may not add another feature setting.
- ADR 0004 remains as historical staged-rollout context but is no longer normative.
