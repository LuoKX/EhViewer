# ADR 0004: Coordinated Rust MVP Rollout

## Status

Superseded by [ADR 0007](0007-single-rust-core-selection.md). This record remains as historical context for the staged 21-setting rollout.

## Context

The migrated MVP capabilities are protected by independent default-off settings.
That is necessary for module-level rollback, but the settings screen currently
requires an operator to coordinate session, catalog, detail, reader, download, and
archive switches one at a time. A partially selected set is valid for diagnostics,
yet it does not provide a reproducible Rust-first MVP configuration or one device
contract spanning those boundaries.

Persisting a second master boolean would create two sources of truth. Computing
effective settings as `master || module` would also prevent an operator from
disabling one problematic module while the master switch remained active.

## Decision

### Android rollout coordinator

`bridge/rollout/RustCoreMvpRollout` owns one explicit list of the twenty-one existing
module settings required for the current MVP path:

- authentication, user configuration, session failure policy, and session lifecycle;
- gallery list parsing, list URL construction/parsing, detail/page navigation,
  detail, comments, previews, token, metadata, and archive-response parsing;
- reader page, MPV token, and reader scheduling;
- download queue, generated-CBZ validation/writing, and ZIP/CBZ reading.

Secondary favorites, rating/vote, image-limit, and torrent settings are not part of
this profile. Their existing switches remain independent.

The coordinator stores no master flag. `setEnabled` updates the explicit list in one
`DataStorePreferences.withMutableSnapshot` transaction, and `isEnabled` returns true
only when every setting in that list is true. The settings screen exposes separate
bulk enable and rollback actions while retaining every module switch underneath.

The transaction does not interrupt platform work. Every later operation or callback
boundary reads the committed settings, so an active download keeps transferring but
its completion callback may select the newly enabled or disabled queue adapter.

### Cross-module device contract

`MvpFlowDeviceTest` temporarily enables the profile, restores the prior preference
snapshot in `finally`, and invokes the real native library across a deterministic
fixture-backed chain:

1. session restore produces an authenticated lifecycle plan;
2. the runtime Gallery List and Gallery Detail selectors consume the canonical
   `eh_net` HTML fixtures through V1 JNI;
3. Gallery Navigation parses one detail and one single-page reference through V1 JNI;
4. Reader Page parsing returns the expected image URL and retry keys;
5. Download Queue reduction starts one parsed gallery, queues another, advances on
   success, and records the second failure.
6. the profile is disabled in one transaction, every listed setting is verified
   false, and a subsequent Gallery Page adapter call returns the legacy result while
   a fail-fast Rust callback remains uncalled.

The Android test asset source includes the canonical `crates/net/test_data`
directory; fixture content is not copied into Kotlin. This is a native boundary
contract plus a deterministic next-operation rollback, not a substitute for startup,
live HTTP, navigation, rendering, active-worker, or physical-device E2E.

## Alternatives Considered

- A persisted master switch was rejected because it would duplicate module state
  and complicate rollback precedence.
- An `OR`-based effective master flag was rejected because an individual module
  could not be disabled during an active rollout.
- Enabling every experimental Rust setting was rejected because several
  secondary features are outside the MVP acceptance chain.
- Changing module defaults now was rejected because live network/UI/worker E2E and
  a physical rollback rehearsal are still incomplete.

## Consequences

- Operators can create or roll back one reproducible MVP Rust configuration without
  racing asynchronous per-setting writes.
- Individual settings remain the authoritative persisted state and the emergency
  rollback mechanism.
- API-35 instrumentation gains a real multi-module JNI/CBOR contract using the same
  offline fixtures as Rust unit tests.
- The same contract proves that a committed bulk rollback is observed by the next
  Gallery Navigation operation without invoking Rust.
- This decision does not prove full MVP E2E and does not authorize deleting legacy
  implementations or changing any module default.
