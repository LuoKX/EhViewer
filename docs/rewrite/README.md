# Rust Rewrite

This directory is the maintainer guide for the Rust-first EhViewer rewrite.
The rewrite keeps Android lifecycle and UI integration in Kotlin while moving
business rules and CPU-intensive work into host-testable Rust crates.

## Documents

- [Architecture](architecture.md): layers, dependency direction, and boundary
  ownership.
- [Style](style.md): formatting, naming, documentation, and comment rules.
- [Testing](testing.md): the critical-feature test pyramid and the mandatory
  95% coverage gates.
- [Migration](migration.md): the slice-by-slice workflow and rollback rules.
- [ADR 0001](adr/0001-rawzip-reader.md): bounded streaming ZIP metadata for
  the transitional archive reader.
- [ADR 0002](adr/0002-ffi-crate.md): versioned JNI/CBOR ownership in the target
  `eh_ffi` crate without changing exported Java symbols.
- [ADR 0003](adr/0003-session-lifecycle.md): Rust-planned session restore,
  authentication handoff, guest entry, and logout with serialized Android effects.
- [ADR 0004](adr/0004-coordinated-mvp-rollout.md): atomic, independently reversible
  activation of the migrated Rust MVP modules and their cross-module device contract.
- [ADR 0005](adr/0005-gallery-navigation.md): versioned, legacy-compatible parsing
  of gallery detail and reader-page navigation references.
- [ADR 0006](adr/0006-reader-fetch-policy.md): pure Reader Fetch policy with
  Android-owned effects, strict post-selection behavior, and deterministic cleanup.

## Non-Negotiable Gates

Every migrated critical feature must:

1. live behind a documented, versioned contract;
2. compile and test without Android when it is domain logic;
3. pass format, lint, rustdoc, and Kotlin style checks;
4. reach at least 95% line and function unit-test coverage in its Rust package;
5. retain an explicit rollback path until Android integration is verified.
