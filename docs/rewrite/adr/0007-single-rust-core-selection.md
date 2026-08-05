# ADR 0007: Rust Is the Sole Production Core

## Status

Accepted on 2026-08-05. This revision supersedes the runtime-selection decision in
the original ADR 0007 and all rollout semantics in ADR 0004.

## Context

The staged rewrite first accumulated module switches and later condensed them into
one master switch. That still left two production architectures, made user-visible
behavior depend on mutable configuration, and retained Kotlin implementations only
for runtime rollback. The portable migration is now complete enough that this
selection layer has no remaining product purpose.

## Decision

### One production architecture

There is no Rust feature switch, backend selector, operation snapshot, or
configuration-controlled legacy path. Every migrated portable parser, planner,
reducer, state machine, archive operation, and validation rule calls Rust directly.
JNI, CBOR, envelope, contract, parser, domain, and resource-limit failures surface
through the existing typed error boundary; production never computes, compares,
replays, or falls back to a Kotlin business implementation.

The retired `use_rust_core*` preference names may remain only in a startup cleanup
list that deletes stale upgrade data. They are not read as values, exposed through
`Settings`, shown in UI, or used by business routing. There is no replacement master
preference.

### Ownership boundary

Rust owns portable business rules, parsers, request/query plans, validation,
filtering/sorting, download and reader policy, archive format handling, and state
machines. Kotlin/Android retains Compose, lifecycle, Room, DataStore, SQLite files,
Paging, WorkManager, notifications, permissions, SAF, platform HTTP transport,
CookieManager, Android object adaptation, and execution of platform transactions and
effects.

This boundary is architectural, not a feature choice: Kotlin may execute a Rust plan
but must not duplicate the plan's business rules. Rust does not acquire Android
`Context`, own platform objects, or execute Android lifecycle effects.

### Compatibility and rollback

Compatibility is protected by frozen fixtures, Rust unit tests, Kotlin/Rust CBOR
goldens, JNI contract tests, and device tests. Historical Kotlin implementations may
remain only as unreachable characterization material while removal is completed.
They are not a production rollback mechanism.

Operational recovery retries or reports the failing operation without changing
backend. Rollback requires installing or reverting to an earlier complete application
version; it is never a runtime setting and must not replay a partially applied effect.

## Alternatives Considered

- Keep one master switch: rejected because it preserves two production architectures
  and makes correctness user-configurable.
- Keep hidden or per-module switches: rejected because mixed ownership and silent
  regressions become valid runtime states.
- Automatically fall back after Rust failure: rejected because it hides broken Rust
  ownership and may duplicate network, database, queue, or file effects.
- Run Kotlin and Rust in production for parity: rejected because it doubles work and
  can consume mutable inputs or execute effects twice.

## Consequences

- Rust is a required packaged dependency and its failures are visible.
- Tests assert the direct Rust contract and absence of fallback rather than backend
  selection behavior.
- New portable business logic belongs in Rust; new runtime Rust switches are forbidden.
- Android-only code remains Kotlin by design and is not counted as incomplete Rust
  migration.
- ADR 0004 and the original switch-oriented wording of ADR 0007 are historical only.
