# ADR 0006: Reader Image Fetch Policy in Rust

## Status

Accepted for the staged Rust rewrite.

## Context

`SpiderQueen` historically owned both the reader-image retry policy and every
Android effect in one coroutine. Cache selection, token acquisition, HTML/API
choice, original-image resolution, image download, cleanup, error retention,
listener notification, and token invalidation were therefore difficult to test
without Android.

Moving the whole coroutine into Rust would cross the wrong boundary. Its URLs,
tokens, gallery/page identifiers, files, locks, network clients, exceptions,
localized messages, and cancellation objects are Android-owned and may contain
sensitive data. The migration needs a pure policy owner while preserving the
legacy effect order and an immediate default-off rollback.

## Decision

### Pure reducer and closed V1 schema

`eh_domain::reader::fetch` owns a deterministic transition reducer:

```rust
pub fn plan_reader_fetch(
    state: ReaderFetchState,
    event: ReaderFetchEvent,
) -> Result<ReaderFetchPlan, ReaderFetchError>;
```

The `readerFetch` V1 request contains only a closed phase, attempt `0..=2`,
stable `force`/`skipHath`/`originalRequested` facts, HTML/API selection facts,
and one closed event. The plan contains the complete next state and ordered
closed actions. No URL, token, path, body, cookie, gallery or page identifier,
exception, display text, credential, or Android object crosses JNI or enters a
Rust diagnostic.

Rust owns these exact policy rules:

- A non-forced request checks cache first; a forced request starts token
  acquisition. A cache hit finishes, and a missing current token fails with the
  Android-owned pToken message.
- Page metadata uses HTML when `forceHtml` is set or no show key is available;
  otherwise it uses the API. An ordinary API transport failure captures no new
  display error, sets `forceHtml`, and advances to the next bounded attempt.
- `skipHath` is valid only with `force`, consumes attempt zero without an image
  download, and forces the next HTML attempt. Original-image resolution appends
  the retained skip key only on attempt one. Selecting a resized image forces
  all later page retries to HTML.
- A recoverable image failure, including an image-location `FatalException`,
  removes intermediates, captures the current display error, and retries until
  attempt two; exhaustion publishes the retained error. A `FileNotFoundException`
  removes intermediates, captures the error, and enters outer classification
  without retry.
- Image-download cancellation removes intermediates and rethrows the same
  cancellation. It never notifies the fatal listener or invalidates a token.
- An outer HTML/API-result/original-resolution failure notifies the fatal
  listener first only when Android classifies it as fatal, then captures its
  display value. Exact Android classification of `Invalid page.` invalidates
  the page token before publishing the retained failure.

### Android effect ownership and selection

`ReaderFetchBridge.kt` maintains an independent Kotlin translation of the
transition table and requires exact plan parity. `selectReaderFetchPlanner`
chooses the implementation and validates the `Start` transition before the
first cache, lock, file, network, listener, or page-state effect. At this
initial-only gate, ordinary `Exception`, `LinkageError`, malformed envelope, or
plan mismatch selects legacy. Fatal `Error` propagates. Once the Rust `Start`
plan is accepted, every later boundary or parity failure is strict: it
propagates without replay, fallback, or implementation switching.

Android remains the sole owner of cache/files, `SpiderDen`, HTTP, URL and token
construction, `Image`, page state, localized errors, listeners, and coroutine
cancellation. Cache bootstrap, current/previous pToken acquisition, and FIFO
throttling complete under `pTokenLock`. That lock returns the still-acquiring
state; only afterward does Android take `showKeyLock`, snapshot availability,
and ask the selected planner for the next transition. Neither lock is held
across the other.

Image failure cleanup and its fixed diagnostic occur in the download catch
before the later planner call, so cleanup is deterministic even if JNI then
fails. The returned plan must contain exactly one leading
`removeIntermediateFiles` action and at least one remaining action.
`consumePreappliedImageCleanup` validates and consumes only that leading action;
all remaining actions keep their original order.

`useRustCoreReaderFetch` is independent, defaults to `false`, and is deliberately
excluded from the coordinated twenty-one-setting MVP rollout. The JNI adapter
adds one exact V1 export, bringing the sorted host manifest to 44 symbols.

## Alternatives Considered

- Moving network, locks, files, or token values into Rust was rejected because
  those are platform effects or sensitive Android state.
- Falling back after any Rust transition was rejected because it can repeat a
  network/file/listener effect under a different implementation.
- Performing image cleanup only after the failure plan returns was rejected
  because a later JNI failure could leave partial files behind.
- Holding `pTokenLock` while reading `showKey` was rejected because it would
  introduce a new nested-lock relationship.
- Adding Reader Fetch to the coordinated MVP profile was rejected because this
  independently reversible slice lacks device execution and rollout evidence.

## Consequences

- Thirteen Rust tests cover the complete reducer table and exact CBOR; the
  production-only CI lane records 201/201 lines and 7/7 functions in
  `eh-domain-reader-fetch.lcov`.
- Twenty-one focused JVM tests cover closed wire shapes, parity, initial-only
  fallback, strict post-selection behavior, cleanup consumption, and sanitized
  diagnostics. Two offline real-JNI device contracts compile.
- API 35 runtime execution remains **UNRUN** on this host. Compilation is not a
  device pass, and no rollout/default or legacy-removal decision follows.
- The new bridge and focused test are Google Style clean. Full
  `spotlessKotlinCheck` still requests a broad legacy whole-file reformat of
  `SpiderQueen.kt`; that narrow, recorded exception is accepted for this slice
  rather than mixing unrelated formatting changes.

## Migration and Rollback

Keep `useRustCoreReaderFetch=false` until API 35 execution, real gallery/archive
fetching, cancellation, retry, cache/file cleanup, lock-order, performance, and
live rollback evidence pass. Switching the flag off restores the unchanged
legacy planner at the next fetch operation and changes no schema or stored data.

File-level rollback removes the reducer/tests, one FFI export and manifest
entry, Kotlin bridge/tests, default-off setting, `SpiderQueen` integration, and
the narrow CI/documentation hunks. The ABI returns from 44 to 43 symbols; rerun
the legacy fetch tests, exact manifest diff, Reader Schedule/Session isolated
coverage, and focused Android compilation after the inverse patch.
