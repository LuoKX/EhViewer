# Migration Workflow

## One Slice at a Time

Each feature moves through the same sequence:

1. Freeze legacy behavior with fixtures and characterization tests.
2. Define the versioned domain request, response, and typed errors.
3. Implement pure Rust behavior with good/base/bad unit tests.
4. Pass format, Clippy, rustdoc, and package-level 95% coverage gates.
5. Add the FFI adapter and Rust/Kotlin golden contract tests.
6. Route the production call site directly through the Rust boundary.
7. Prove the Kotlin adapter validates the complete Rust result before platform effects.
8. Remove production parity, selectors, and automatic fallback before marking the slice complete.

## Slice Exit Criteria

- All documented behavior and errors are implemented.
- The critical Rust package meets line and function coverage of at least 95%.
- No live network appears in unit tests.
- JNI/CBOR round-trip and malformed-input tests pass.
- Android host build, JNI/CBOR, ABI, and instrumentation-source evidence exists.
- Schema/data compatibility is preserved, and device execution is recorded separately
  from host-only evidence.

## Favorite Mutation Planning Slice

Rust plans one single or bulk request with ordered form repetition. Production invokes
and validates Rust before HTTP; mismatch, exception, or linkage failure is surfaced.
Legacy equivalence is asserted only in offline characterization fixtures.

## Gallery Archive Request Planning Slice

`eh_net::gallery::archive_request` now plans the archive-list GET target and archive
download POST target, referer, optional origin, and ordered direct/H@H form fields.
Android still owns funds lookup, HTTP, the direct-download retry delay, response parsing,
file download, and UI effects. Production builds and validates only the Rust plan
before the first HTTP call; failures surface without fallback. Rust and Kotlin share exact list/download request and response CBOR
goldens. The isolated Rust gate is 100% line (237/237) and function (11/11), focused JVM
coverage is 8/8, and the exact V1 manifest is 41/41. The real-JNI list/download device
test compiles but has not run in the current non-KVM environment.

## P1 Audit Follow-up

The four portable gaps from the follow-up audit now have direct Rust owners:
tag dictionary/index/scoring, EH recovery planning/reconciliation, Gallery request
and Detail/reader/MPV URL planning, and language/metadata inference. See
[P1 migration](p1-migration.md) for boundaries, cache compatibility, budgets,
independent coverage and device-execution status. P2 policies/codecs/file rules
remain Kotlin-owned pending their own slices. This is not a claim that every
portable line in the project has moved to Rust.

## Planned Order

1. Contract/domain foundation.
2. Session and gallery list.
3. Gallery detail and reader metadata.
4. Download/archive state machine.
5. Secondary features and legacy removal.

## Current Mainline Status

As of 2026-08-13, production has no Rust runtime selector. Portable parser, planner,
reducer, reader, download, archive, session, search, favorite, settings, and daily-check
rules invoke Rust directly. Android-only platform ownership remains Kotlin by design.
Rust Core health is a required diagnostic, not a selectable feature.

### Mainline Semantics

Each operation uses one Rust implementation without shadow calculation, parity,
automatic fallback, or replay. The retired master/module key names remain only in
startup cleanup and never decide behavior. Later module status sections preserve
historical implementation facts and test counts; their former switches and rollback
language do not describe the current runtime.

The gallery-list parser was the first migrated behavior slice. Its implementation
now lives in `eh_net::gallery::list`, uses checked-in offline fixtures, and
returns a V1 `eh_domain::ApiEnvelope` with typed errors through
`parseGalleryInfoListV1`. The original unversioned symbol and its frozen root parser
have been removed and are covered by a retired-symbol negative manifest.

### Compatibility Exceptions Still Open

- The root `ehviewer_rust` crate is a packaging facade, not a compatibility backend.
  It retains only `JNI_OnLoad`, feature forwarding, and the `eh_ffi` static-link edge.
  It does not yet inherit the workspace `missing_docs`/`unsafe_code` lints, but still
  runs rustfmt and host/Android-target Clippy with warnings denied. No business
  behavior may be added there.
- The former master/module key names are compatibility data for startup deletion
  only; they are not read as selectable production state.
- Rust and Kotlin share checked-in hexadecimal CBOR golden fixtures. Kotlin request
  bridges use one definite-length `rustCbor` configuration matching `serde_cbor`,
  and `eh_ffi::codec` tests protect atomic output on undersized buffers. Android host and
  instrumentation sources compile locally. A local API 35 x86_64 software-TCG
  attempt reached AOSP ATD boot, but its network stack and `system_server`
  restarted before APK installation, so the connected task executed zero tests.
  The xTS CPU profile requires KVM; successful device execution therefore remains
  with the KVM-enabled CI/device gate.
- Device integration/E2E evidence is tracked independently from host evidence. No
  backend setting is exposed and no legacy production mode remains.

The structural FFI migration owns the checked versioned business symbols, unified
archive V2 reader symbols, archive writer/validator symbols, Rust Core health, the
animated WebP registry, Android Bitmap/HardwareBuffer adapters, and shared checked
transport. Current manifests require exactly 60 V1, 13 Archive V2, nine Android
image, three host WebP, one Hash compatibility, and one `JNI_OnLoad` export. Negative
manifests reject all fourteen former root parser exports, all three retired GIF
exports, all nine old C Archive exports, and `strnatcmp`. Older counts below are
historical evidence only.

The root parser tree and its live-network test are removed. The app-owned C Archive
wrapper, Kotlin declarations, natsort code, private header, and GIF C implementation
are also removed. The root static library now keeps only `JNI_OnLoad` and links
`eh_ffi`; CMake keeps a behavior-free translation unit plus the pinned libarchive,
libwebp, and system libraries required by Rust adapters.

The Android 9–13 GIF delay workaround is now Rust-owned in `eh_image::gif`. The JNI
adapter returns a bounded JVM-owned snapshot without moving the source descriptor;
the old app-owned `gifutils.c` mmap/munmap boundary and its three retired exports are
removed and covered by positive image plus negative retired-symbol manifests. Kotlin
retains only API selection, descriptor acquisition, Coil routing, and source lifetime.

Gallery title/category compatibility is now canonical in
`eh_domain::gallery_rules`. Gallery List/detail/metadata and catalog URL modules use
its one category bit/alias/inversion owner. Similar-gallery search uses one
`galleryTitle` V1 plan preserving the former Kotlin regex, chapter wildcard,
last-pipe, trim, and empty rules. Kotlin retains display labels/colors, title
preference, filenames, Compose state, and navigation; the dead string-category,
mask-inversion, and title-cleanup production helpers are removed.

The former per-module rollout is recorded by superseded
[ADR 0004](adr/0004-coordinated-mvp-rollout.md). The active decision is
[ADR 0007](adr/0007-single-rust-core-selection.md): Rust is the sole portable
production core, with no runtime setting, parity, or fallback. The old API-35
21-setting chain remains historical native-boundary evidence only.

## Historical Slice Record

The sections below record how individual slices were introduced. Any `default-off`,
`useRustCore*`, global off/on, parity, fallback, rollback-switch, or deferred-device
wording is historical and superseded by the current mainline status above. Ownership,
wire contracts, fixtures, and measured results remain useful evidence.

## Rust Core Health Foundation

The A1 foundation adds one versioned, always-available health request before any
startup integration. Rust accepts exactly `{ version: u8, nonce: i64 }`; V1 returns
module `rustCore`, envelope version `1`, payload `protocolVersion = 1`, and the exact
signed nonce. Unsupported request versions return the fixed typed error
`unsupported Rust Core health request version` without reflecting request data.

`eh_ffi::health` owns the pure protocol and is compiled independently of the JVM
feature. `eh_ffi::jvm::health` owns the sole
`Java_com_hippo_ehviewer_bridge_RustBridgeKt_nativeRustCoreHealthV1` export and reuses
the checked direct-buffer decoder plus atomic encoder. Kotlin `RustBridge` exposes
`isReady()`, `checkVersion(expectedVersion)`, and `ping(nonce)`: readiness converts
`Exception` and `LinkageError` to `false`, strict methods surface failures, and fatal
`Error` always propagates.

This foundation deliberately adds no startup or `EhApplication` call, no
`JNI_OnLoad` change, no preference, no logging, and no module rollout switch. Rust
good/base/bad and byte-exact CBOR tests pass 3/3 with an independent 100% line
(74/74) and function (5/5) gate; Kotlin JVM contracts pass 9/9. The real-JNI
instrumentation source compiles, while API 35 runtime execution remains UNRUN. The
host V1 manifest is exact at 43/43. Rollback deletes the five health source/test files,
removes the narrow module/CI/documentation hunks and one symbol line, and restores
the prior 42/42 manifest.

Gallery-list request construction now has a separate
`eh_net::gallery::list_url` slice. Its V1 DTO covers both sites and all seven
`ListUrlBuilder` modes while Rust preserves guest-category correction, category
inversion, language prefixes, pagination, advanced filters, path encoding, and
query order. Kotlin still owns the mutable serializable builder, settings,
QuickSearch/SavedState, language indices, and HTTP execution. The default-off
adapter computes the frozen legacy URL first and accepts Rust output only after an
exact equality check, so any boundary or encoding mismatch returns the already
validated legacy target. Targeted Rust line/function coverage is 100%.
Secret-bearing `EhUrl` APIs, device/performance evidence, rollback rehearsal, and
removal of the transitional double construction remain open.

Incoming gallery-list URL interpretation now has its own
`eh_net::gallery::list_url_parser` V1 slice. Android keeps Ktor syntax handling and
the exact E/EX host gate, and forwards only two decoded path segments plus first
values for the 18 query fields consumed by the frozen parser. Rust preserves root
image-search presence semantics, uploader/tag second decoding, toplist whitelist,
category aliases, query defaults, and advanced-search bit parsing. The original
URL and ignored suffixes never cross JNI. The default-off adapter runs legacy
first and accepts Rust only after full builder-state equality. Targeted coverage
is 99.57% line and 100% function (99.52% region); Android host/device and rollback
evidence remain open.

Gallery detail and reader-page navigation references now share the independent
`eh_net::gallery::navigation` V1 slice. The request selects detail or page parsing,
carries the raw candidate plus the frozen strict/loose mode, and returns a closed
no-match/detail/page plan. Rust preserves the legacy case-sensitive substring
grammar for E/EX, public `/lofi`, `/g`, `/mpv`, and `/s`, while rejecting inputs
larger than 16 KiB and treating integer overflow as no match without exposing URLs
or tokens in diagnostics. The default-off `useRustCoreGalleryNavigation` adapter
runs the side-effect-free legacy parser first and adopts Rust only after complete
plan equality; unsupported input never enters JNI and failure returns legacy before
navigation. Rust/Kotlin CBOR goldens, parser-selection tests, the then-exact 39-symbol
manifest, and API-35 MVP JNI calls are checked in. Targeted coverage is 98.80% line
(247/250) and 100% function (21/21). A deterministic Activity test now asks Android
to resolve package-scoped HTTP/HTTPS intents for both gallery hosts, rejects an
unknown host and FTP, launches an implicit page-zero `ACTION_VIEW`, follows the real
intent channel and Compose destination, and stops at the documented `pageIndex = -1`
sentinel before HTTP.
System-wide verified-domain default routing, clipboard dispatch, positive-page
network navigation, performance, and active/physical rollback evidence remain open.

Remote Favorites read/list URL construction is independently owned by
`eh_net::gallery::favorites_url`; it preserves the public/restricted host,
`-1`/`0..9` category mapping, invalid-category omission, search and cursor blank
rules, and exact query order. The separate
`eh_net::gallery::favorite_mutation` owner builds pure single/bulk mutation
plans with the exact URL, referer, origin, and ordered/repeated form fields.
Android retains the `-2` local category, Room/local searches, mutable builder
and settings, mutation inputs, HTTP/Cookie/session handling, response
parsing/handling, refresh/UI/DB/delay sequencing, and all effects. Local lists
never cross JNI; planner selection and fallback happen before HTTP, and the
default-off remote adapter accepts Rust output only after exact comparison with
the frozen legacy plan. Targeted Rust line/function/region coverage is 100%.
Device/performance and rollback evidence and removal of transitional double
construction remain open.

Remote Favorites response parsing now has its own
`eh_net::gallery::favorites` V1 slice. It freezes source-order `.fp` traversal,
the pre-parse ordinal-10 skip, strict untrimmed signed-`i32` counts,
XML-decoded/trimmed names, per-row drops, and the exact ten-pair panel invariant,
then reuses the Gallery List parser and its typed errors for the page payload.
Invalid panels return one fixed body-free `galleryFavorites` failure, while safe
child access closes the legacy index-panic path. Android retains GET and completed
mutation HTTP execution, cookies, local favorites, paging state, Room enrichment,
mutation inputs, refresh, and UI. The independent default-off
`useRustCoreGalleryFavorites` switch selects exactly one parser for the completed
response; V1 failure never replays a request or the consumed buffer. Offline
good/base/bad fixtures, legacy characterization, a Rust-generated Kotlin-decoded
CBOR golden, typed error mapping, cancellation, and parser-selection tests are
checked in. Targeted coverage is 99.41% line (169/170), 100% function (18/18), and
97.81% region; the complete DefaultDebug JVM suite passes locally, while real
GET/mutation, device performance, and rollback evidence remain open.

The authentication parser slice now lives in `eh_net::auth`. It parses sign-in
responses, profile URLs, and profile pages into V1 contracts with checked-in
fixtures and Rust/Kotlin golden CBOR tests. A separate default-off setting keeps
the legacy Kotlin sign-in parser and legacy Rust profile parser available.
Passwords, cookies, WebView fallback, HTTP execution, and CookieManager storage
remain Android-owned and never enter Rust DTOs. Cookie/secret ownership remains a
later decision; the adjacent lifecycle slice migrates only sanitized planning and
concurrency-safe Android orchestration.

The adjacent `eh_domain::session` slice now classifies only sanitized failed
response facts. It preserves the legacy Cloudflare, empty-body/Sad-Panda,
GalleryUnavailable, HTTP-status, typed-error, non-HTML, parse-error, and rethrow
precedence without receiving URLs, cookies, headers, credentials, or response
bodies. `EhEngine` uses the V1 plan behind default-off
`useRustCoreSessionPolicy`; cancellation bypasses JNI, planning failure uses the
untouched legacy mapper, and a validated decision is never replayed. Targeted
coverage is 100% line/function. Cookie lifecycle, login redirect state updates,
and WebView effects remain outside this failure-policy module.

The lifecycle contract frozen in [ADR 0003](adr/0003-session-lifecycle.md) is now
implemented. `eh_domain::session::lifecycle` plans restore, successful authentication,
guest entry, and logout without receiving cookie values or credentials. Android
keeps CookieManager, Settings, HTTP, and WebView effects behind one serialized,
generation-checked controller. Cookie deletion becomes awaitable, `postLogin`
becomes structured instead of global, and stale profile/login completions are
rejected. Rust/Kotlin CBOR goldens, fake-store race/rollback tests, the real JNI
instrumentation source, and an independent 100% line/function domain gate are in
place. The rollout remains default-off pending the first API 35 device execution,
real login redirect/WebView/Cookie callback E2E, performance, and an explicit
rollback rehearsal.

Gallery Detail now lives in independently gated
`eh_net::gallery::{preview,comments,detail}` modules. The rich parser consumes one
DOM and returns metadata, tags, comments, previews, and event-pane HTML through a
V1 `galleryDetail` envelope; the smaller preview and comment entry points retain
their own V1 envelopes. Checked-in offline fixtures, Rust/Kotlin golden contracts,
dedicated JNI symbols, and default-off settings leave every original unversioned
parser available for rollback. Device integration/E2E evidence is still required
before enabling the rich-detail path by default or deleting legacy code.

The first Reader/Image slice now lives in `eh_net::reader::page`. It parses the
normal HTML and backslash-escaped `showpage` API shapes into a V1 `readerPage`
contract containing image URLs and retry keys. `EhEngine` selects this ByteBuffer
path only when its default-off setting is enabled; otherwise both direct-page and
API requests keep the original Kotlin parser.

Reader MPV token extraction now also lives in `eh_net::reader::mpv`. It preserves
the first marker/semicolon boundary, strict JSON array and string-field behavior,
page order, duplicates, empty tokens, Unicode escapes, and unknown sibling fields.
The V1 parser explicitly rejects the legacy markerless fixed-offset substring bug
and returns only a sanitized `readerMpv` error. Android still owns the MPV request,
cookies, `SpiderInfo` token map, page-index mapping, page URL/API construction, and
the cached-token/MPV/preview fallback chain. `useRustCoreReaderMpv` is default-off
and selects the text or direct-buffer representation before one HTTP execution;
V1 failures never issue a second legacy request. Offline good/base/bad fixtures,
a Rust-generated Kotlin-decoded CBOR golden, and parser-selection tests are
checked in. Targeted coverage is 100% line (103/103) and 100% function (13/13);
the complete DefaultDebug JVM suite passes locally, while real MPV/page ordering,
fallback, performance, and rollback evidence remain open.

Gallery Token API response parsing now lives in `eh_net::gallery::token`. It
decodes every `tokenlist` item before returning only the first token, requires each
source gid to fit signed 64-bit range but deliberately does not compare or expose
it, and preserves empty, opaque, whitespace, and Unicode token strings. Valid
fallback objects and malformed/non-object responses use two fixed `galleryToken`
errors; response bodies, remote error text, and tokens are never copied into those
errors. Android retains the `gtoken` request body, existing token, page conversion,
HTTP/site state, and navigation. `useRustCoreGalleryToken` is default-off and
selects text legacy or direct-buffer V1 parsing before one request execution; V1
failure never issues a legacy replay. Offline good/base/bad tests, legacy fallback
characterization, a Rust-generated Kotlin-decoded CBOR golden, and parser-selection
tests are checked in. Targeted coverage is 100% line (96/96) and 100% function
(12/12); the complete DefaultDebug JVM suite passes locally, while real
API/navigation, performance, and rollback evidence remain open.

Gallery Tag Vote response parsing now lives in `eh_net::gallery::tag_vote`, with the
row grammar shared explicitly through `eh_net::gallery::tags`. A string service
`error` retains precedence and remains user-facing; every malformed response or tag
pane uses one fixed body-free failure. Source order, alias stripping, entities, power/vote
classes, unknown marker classes, and the legacy empty-tag-text behavior are frozen,
while Gallery Detail keeps its stricter non-empty text rule. Android retains API
credentials, gallery keys, submitted tag/vote values, HTTP, and UI mutation. The
default-off `useRustCoreGalleryTagVote` switch selects the existing unversioned Rust
JNI parser or workspace V1 parser before one request execution, with no replay after
V1 failure. Offline good/base/bad fixtures, shared-grammar tests, a Rust-generated
Kotlin-decoded CBOR golden, envelope mapping, cancellation, and parser-selection tests
are checked in. Targeted coverage is 97.59% line (283/290) and 100% function (36/36);
the complete DefaultDebug JVM suite passes locally, while real API/UI, performance,
and rollback evidence remain open.

Gallery Rate response parsing now lives in `eh_net::gallery::rate`. It preserves
unknown fields, last duplicate-field wins, every signed 32-bit count, and every
JSON number or quoted numeric string whose `f32` narrowing remains finite; it deliberately adds no
rating-range or non-negative-count rule. Missing, null, wrong, non-object,
malformed, overflowed, and non-finite shapes use one fixed body-free
`galleryRate` failure, while an unrelated service `error` remains ignored when
both legacy success fields are valid. Android retains credentials, gallery
keys, half-step request rounding, HTTP/session state, and UI mutation. The
default-off `useRustCoreGalleryRate` switch selects text legacy or direct-buffer
V1 parsing after constructing one request, and V1 failure never replays the
rating POST. Offline good/base/bad and duplicate-field tests, a Rust-generated
Kotlin-decoded CBOR golden, typed envelope mapping, cancellation, and
parser-selection tests are checked in. Targeted coverage is 100% line (93/93)
and 100% function (10/10); the complete DefaultDebug JVM suite passes locally,
while real API/UI, device performance, and rollback evidence remain open.

Gallery List `gdata` response parsing now lives in
`eh_net::gallery::metadata`. The slice preserves full-response validation,
source order and duplicate IDs, last duplicate JSON fields, unknown-field
ignoring, missing/null uploaders, signed timestamp/page boundaries, and every
finite `f32` rating. It performs only the frozen known-entity, category, and
thumbnail normalization; unknown/nested entities remain untouched, category
labels are never trimmed, and `t/` is preserved after `ehgt.org/` or on relative
keys. Android retains request construction and gallery
tokens, 25-item/five-request batching, HTTP/session state, first-target matching,
date formatting, language derivation, and mutation. The independent default-off
`useRustCoreGalleryMetadata` switch selects text legacy or direct-buffer V1 for
one already-created request; V1 failure never replays it. Offline good/base/bad,
duplicate-field and normalization tests, a Rust-generated Kotlin-decoded CBOR
golden, typed envelope mapping, cancellation, first-match/source-order mutation,
and parser-selection tests are checked in. Targeted coverage is 100% line
(218/218) and 100% function (19/19); the complete DefaultDebug JVM suite passes
locally, while real API/device performance and rollback evidence remain open.

Gallery Comment Vote response parsing now lives in
`eh_net::gallery::comment_vote`. It preserves required signed `comment_id`,
`comment_score`, and `comment_vote` widths, unknown fields, final duplicate-field
values, quoted integer strings, and the legacy behavior of ignoring an unrelated service `error` when
all success fields are valid. It deliberately adds no positive-id,
vote-direction, non-negative-score, or request-identity rule. Every invalid
shape uses one fixed body-free `galleryCommentVote` failure. Android retains
credentials, request identifiers and submitted vote, HTTP/session state,
comment refresh, and UI messaging. The independent default-off
`useRustCoreGalleryCommentVote` switch selects text legacy or direct-buffer V1
parsing after constructing one request; V1 failure never replays the mutation
POST. Offline good/base/bad and duplicate-field tests, a Rust-generated
Kotlin-decoded CBOR golden, typed envelope mapping, cancellation, and
parser-selection tests are checked in. Targeted coverage is 100% line (103/103)
and 100% function (12/12); the complete DefaultDebug JVM suite passes locally,
while real API/UI, device performance, and rollback evidence remain open.

Reader request scheduling now also has a pure `eh_domain::reader` slice. V1
commands cover request/cache-hit, forced retry, and restart while ordered actions
preserve the legacy `PageLoader` sequence and directional prefetch bounds. Kotlin
retains every image, cache, flow, decode job, archive/network request, and applies
only a fully validated plan. Cached images are pinned across planning; fallback is
allowed only before action application. `useRustCoreReaderSchedule` remains
default-off. Local targeted coverage is 100% line and 100% function, and the
complete DefaultDebug JVM suite passes locally. Real gallery/archive reading,
rapid direction changes, cache eviction, retry,
restart, performance, and rollback still require device evidence.

Reader image fetching now has a separate `eh_domain::reader::fetch` policy slice.
Rust receives only a closed V1 state plus one sanitized event and returns the next
state with ordered Android effects. It preserves cache-first versus forced start,
attempts `0..=2`, HTML/API selection, attempt-zero H@H skip, attempt-one original
skip-key behavior, resized-to-HTML retries, retained display errors, fatal outer
notification, exact invalid-page token invalidation, and distinct recoverable,
`FileNotFoundException`, and cancellation paths. Android still owns identifiers,
URLs, tokens, files, HTTP, locks, `SpiderDen`, page state, listeners, messages, and
exceptions.

The operation-scoped global snapshot selects one complete Reader Fetch planner before
the first effect. Global on validates the Rust `Start` plan and every later Rust
transition strictly; failures propagate without legacy selection, replay, or
switching. Global off executes only the legacy planner. pToken acquisition and throttling finish under `pTokenLock` before the
separate `showKeyLock` snapshot. Image-failure cleanup and logging happen before
the fallible later JNI transition, whose single leading cleanup action is validated
and consumed before the remaining actions execute.

The recorded Rust/JVM/coverage and ABI counts in this historical slice predate C6
and must be refreshed by C7. The offline real-JNI test source compiles, but API 35
execution remains UNRUN and is not a device pass. Real gallery/archive fetching,
cancellation, cache/file cleanup, performance, and live rollback remain deferred
device evidence.

Border detection, QR finder-pattern detection, and Android pixel normalization
live in the host-only `eh_image` crate. `eh_ffi::jvm::bitmap` now owns the unchanged
`detectBorder(Bitmap)` and `hasQrCode(Bitmap)` JNI symbols. Android storage remains
behind an RAII lock guard; padded bitmap rows are copied into aligned tight storage
before host algorithms run. Empty images return a contained border error and a
negative QR result instead of reaching unchecked assumptions.

The `copyBitmapToAHB` JNI symbol is also unchanged, but its crop/stride algorithm
now lives in safe `eh_image::region`. The host module validates non-empty layouts,
pixel size, row stride, storage length, source bounds, destination capacity, and
all byte-offset arithmetic before copying padded rows.
`eh_ffi::jvm::hardware_buffer` checks signed coordinates, one-layer AHB usage, exact
Bitmap/AHB format parity, pointer ranges, and disjoint storage. RAII guards balance
both locks on success, error, and panic paths. A checked-in API 29+ instrumentation
test exercises all three packed formats, repeated lock cycles, invalid-coordinate
containment, hardware-bitmap wrapping, and closing the caller's Java buffer before
readback. CI runs it on an API 35 x86_64 emulator; the first successful remote run
remains required evidence before this slice can change rollout defaults.

Animated WebP now lives in `eh_ffi::webp`, with JVM create/info/destroy exports in
`eh_ffi::jvm::webp` and Android frame copying in `eh_ffi::jvm::bitmap`. Rust copies
the remaining Java buffer, stores decoders behind positive opaque handles in a
synchronized registry, checks every libwebp status, and makes destroy idempotent.
The Android adapter validates RGBA format, dimensions, stride, and pointer
arithmetic before copying each row. Kotlin construction releases partial
allocations on failure, heap/direct inputs preserve their remaining-byte range, and
`dispose()` waits for decoding to stop before recycling both frame bitmaps. The
checked-in ten-frame fixture now covers success/reset/ownership, callback failure,
independent handles, invalid data/handles, metadata bounds, and poisoned locks under
an independent 99.63% line/100% function gate. API-35 instrumentation constructs,
draws, starts, stops, and idempotently disposes the real animation; its first remote
execution remains open.

The first Download/Archive slice now owns queue decisions in
`eh_domain::download`. V1 commands cover start-one/range/all, idle activation,
stop-one/range/all, and terminal success/failure while preserving the legacy FIFO
selection rules. Rust also normalizes volatile persisted states after process
death. Separate reducer and recovery JNI symbols use owned CBOR requests and
atomic response buffers; Kotlin validates entry identity/order, exact changes,
effects, and Rust/Kotlin request/response golden fixtures before touching Android
state.

`DownloadManager` invokes this reducer only behind the default-off
`useRustCoreDownloadQueue` switch. Queue commands and deletion are serialized in
`sortMutex -> mutex` order. Android continues to own Room, `SpiderQueen`, speed
tracking, service callbacks, progress counters, paths, and archive-file
shortcuts. Recovery resets only volatile rows outside the live current/FIFO
topology, so switching from the legacy scheduler cannot erase a valid worker.
The legacy scheduler remains the runtime rollback path. Android device E2E
and an explicit rollback rehearsal remain open before enabling this path by
default.

Remote archive-response parsing now has a separate
`eh_net::gallery::archive` V1 slice. It preserves direct/H@H option ordering,
legacy display fields, optional externally supplied funds, the pending-target
retry signal, and typed NoHathClient/InsufficientFunds errors. Final targets must
be absolute HTTPS URLs without credentials or fragments and with exactly one
`start=1`; unrelated query fields remain intact. The default-off
`useRustCoreGalleryArchive` switch selects the full V1 list/target path before the
body buffer is consumed. Security or typed Rust failures never fall through to
the legacy arbitrary-href concatenation. Rust/Kotlin list and target CBOR goldens
are checked in. Targeted coverage is 97.75% line and 100% function; Android host
tests pass locally, while device, real download, and rollback evidence remain open.

Image-quota parsing now has an independent `eh_net::home` V1 slice. It preserves
the legacy three-value source order, comma removal, signed 32-bit filtering, and
the single-value `maximum=0/-1` sentinel selected by the exact case-sensitive
`No restrictions` marker. Android still owns the home/funds requests, reset POST,
torrent key, cookies, credentials, and UI composition. The default-off
`useRustCoreImageLimits` switch selects one parser before its direct buffer is
consumed; V1 failures never replay the mutated input through legacy code. A
Rust-generated/Kotlin-decoded CBOR golden and adapter-selection tests are checked
in. Targeted coverage is 96.90% line and 100% function; Android host tests pass
locally, while real device behavior remains open.

News event-pane parsing now has an independent `eh_net::news` V1 slice. It returns
`NewsPage(eventHtml?)`, preserving missing versus empty panes plus nested inner HTML
and entity text. Gallery Detail reuses this one `#eventpane` extractor. Android
retains HTTP, the Cookie star side effect, daily scheduling/notifications, and the
`parse=false` request-only path. The independent default-off `useRustCoreNews`
switch selects one parser before buffer consumption and is deliberately excluded
from the twenty-one-setting MVP rollout; cancellation and selected V1 failures do
not trigger legacy replay. Rust good/base/bad and CBOR-golden tests, Kotlin envelope,
typed-failure/cancellation/selection tests, the exact 39-symbol manifest, and a
compiled and packaged real-JNI API-35 contract are checked in. Targeted coverage is 98.46% line
(64/65) and 100% function (9/9); its first KVM-enabled or physical-device execution
remains open.

User-configuration favorite-category parsing now has an independent
`eh_net::user_config` V1 slice. It preserves the exact legacy attribute sequence,
single ASCII index digit, non-empty value rule, source order, duplicate indexes,
whitespace, supported entity decoding, and per-value decode drops. Empty or
partial arrays remain successful Rust payloads; Android's existing ten-item
`Settings.favCat` setter remains the final mutation invariant. Android also keeps
HTTP, the restricted-site redirect retry, cookies, credentials, and preference
storage. `useRustCoreUserConfig` is default-off and chooses one parser before its
buffer is consumed. Rust/Kotlin CBOR golden and adapter-selection tests are
checked in. Targeted coverage is 100% line and 100% function; Android host tests
pass locally, while real device behavior remains open.

Gallery torrent-list parsing now has an independent `eh_net::torrent` V1 slice.
It preserves the frozen table grammar, source order, case-sensitive `Expunged`
filter, display fields, entity decoding, and per-row malformed/overflow skip
behavior. V1 never exposes the raw `onclick` URL: a row survives only when its
final slash-delimited segment is exactly a 40-ASCII-hex hash plus lowercase
`.torrent`, and the hash is normalized to lowercase. Relative and dynamic absolute
prefixes remain compatible without an unevidenced host allowlist; query, fragment,
suffix, length, and character violations drop that row. Android retains HTTP,
torrent/tracker keys, magnet `Intent`, clipboard fallback, and UI. The default-off
`useRustCoreTorrent` switch selects one parser before buffer consumption, and V1
failures never fall through to the legacy raw-URL path. Rust/Kotlin CBOR golden,
legacy missing-field decode, security, and adapter-selection tests are checked in.
Targeted coverage is 96.45% line (136/141) and 100% function (15/15); Android host
tests pass locally, while real device behavior remains open.

The Download/Archive validation slice adds host-only `eh_archive` validation for CBZ
files generated by `SpiderDen`. It enforces sequential page names, one final
`ComicInfo.xml`, regular non-empty unencrypted Stored entries, and reads every
payload to EOF for CRC verification. The Android V1 adapter synchronously borrows
a descriptor through a duplicate, restores its seek position, and exposes no
path to Rust. Validation runs after truncate-mode archive writing and before the
existing completion callback. Failure or cancellation removes the generated CBZ
while retaining source pages. `useRustCoreArchiveValidation` is default-off for
immediate rollback. Physical/SAF device evidence remains open.

Generated CBZ writing uses the `eh_archive::writer` slice. It validates
the complete ordered name/count contract before output, streams each non-empty
page plus final `ComicInfo.xml` as Stored mode `0644`, reports every input/output/
finalization failure, and requires only `Read`/`Write` rather than paths or a
seekable output. JNI duplicates Android-owned descriptors and returns the
uncompressed byte summary. The global operation snapshot chooses Rust or legacy
`archiveFdBatch` exactly once; a selected Rust failure never retries legacy on
partial output. Rust good/base/bad and
round-trip validation tests plus Kotlin input/summary/selection/no-fallback tests
are checked in. Writer coverage is 100% line (227/227), 100% function (24/24),
and 95.38% region; aggregate `eh_archive` coverage is now 98.47% line (1350/1371)
and 100% function (127/127). The complete DefaultDebug JVM suite and Android
instrumentation-source compilation pass. CI now runs a real descriptor writer/
validator round trip and failure cleanup on its emulator; physical/SAF device
writing, performance, and rollback rehearsal remain open.

The reader-side slice is now the unified V2 lifecycle in
`eh_ffi::jvm::archive::archive_native`, backed by the pinned libarchive codec. It
detects ZIP/CBZ, RAR/RAR5, 7z and tar, handles encrypted ZIP passwords, filters the
same eleven lowercase playable extensions, applies natural ordering, and enforces
100,000 entries plus 512 MiB per image. Unsupported codec encryption or compression
fails in Rust without legacy fallback.

Positive opaque handles serialize reader state and make close idempotent while
waiting for active extraction. JNI duplicates source/output descriptors, restores
the shared source offset, and writes only Java-owned direct buffers or duplicated
output descriptors. The global snapshot sends every archive to V2 when on and uses
the retained legacy C reader only when off. Historical `eh_archive::reader` rawzip
tests remain characterization evidence, but the dead V1 reader JNI exports have
been removed. C7 requires fresh multi-format, password, corruption, descriptor,
concurrency, ABI, and packaging evidence; physical/SAF execution remains deferred.

## Download Progress

`eh_domain::download::progress` owns only immutable runtime counter planning:
reset, page totals/results, signed JVM-compatible ETA arithmetic, and terminal
legacy/state projection. Android retains Room, workers, transfer maps, locks,
SpeedTracker, SpiderQueen, listeners, files, and all effects. The operation-scoped
global snapshot selects one planner: global off executes only legacy, while global
on invokes and validates only Rust. Boundary or contract failures surface before
effects; no parity, fallback, or replay is allowed.
Each callback captures its expected `DownloadInfo` at receipt. Enabled mode locks
`sortMutex -> mutex`, checks object identity, and takes the transfer mutex last;
it never acquires queue locks while holding the transfer mutex. Stale callbacks
are ignored before mutation with a fixed identifier-free diagnostic, while the
disabled path keeps the legacy lock-free queue behavior. Reset and Finish stay
under queue locks. Finish validates progress state against the queue reducer's
final entry before speed-tracker cleanup, detach, persistence, listeners, or
worker advancement.

At the historical Download Progress slice completion, the V1 ABI was exactly 42/42
symbols. The then-current production-only coverage was queue line
97.05% (329/339), function 100% (28/28); progress line 97.26% (71/73), function
100% (2/2); and aggregate `eh_domain` line 99.35% (1975/1988), function 100%
(109/109). C7 must refresh these counts against the single-switch implementation.
API35 device execution is UNRUN on this host.
