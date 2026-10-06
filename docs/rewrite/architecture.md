# Rewrite Architecture

## System Shape

```text
Compose UI and Android services
              |
       Kotlin bridge adapters
              |
     versioned JNI/CBOR boundary
              |
                         ffi
        /        /        |         \        \
      net     image    archive    domain    store
```

The arrows point toward dependencies: `ffi` may depend on `net`, `image`,
`archive`, `domain`, and `store`; those crates never depend on `ffi` or Android.
The `ehviewer_rust` static library is the Android packaging/link facade and retains
`JNI_OnLoad`; portable production behavior is owned by the workspace crates.
Versioned domain/network and archive-descriptor adapters, the Rust Core health
boundary, and Android Bitmap, HardwareBuffer, and animated WebP JNI compile from
`eh_ffi`. The root facade is not an alternate production business backend.

All Kotlin request DTOs crossing this boundary use the shared definite-length
`rustCbor` configuration. Rust `serde_cbor` output and checked-in golden fixtures
therefore define one stable byte-level protocol instead of relying on library
defaults that may change between Kotlin serialization releases.

## Crate Responsibilities

| Directory | Crate | Owns | Must not own |
| --- | --- | --- | --- |
| `crates/domain` | `eh_domain` | DTOs, errors, state machines, use cases | JNI, Android, HTTP, files |
| `crates/archive` | `eh_archive` | Generated-CBZ writing/validation, bounded ZIP reading, recovery entry policy | JNI, Android, paths, lifecycle |
| `crates/image` | `eh_image` | Pixel normalization, border/QR analysis, safe region copies, GIF recognition/rewrite | JNI, Android bitmap ownership, rendering |
| `crates/net` | `eh_net` | Sessions, requests, parsers | UI and Android lifecycle |
| `crates/store` | `eh_store` | Persistence ports/adapters | UI and parser rules |
| `crates/ffi` | `eh_ffi` | JNI, CBOR, Rust Core health, unified archive-reader lifecycle, panic containment | Android lifecycle and unrelated business decisions |
| `crates/testkit` | `eh_testkit` | Fixtures, fakes, golden contracts | Production behavior |

Crates are added when they have real responsibility. Empty layering and generic
utility crates are not allowed.

## Boundary Rules

- DTOs are versioned and serialized in one owner module.
- Rust and Kotlin wire types require golden round-trip tests.
- The FFI layer validates pointers, buffer length, input encoding, and protocol
  version before calling business code.
- Panics and Rust implementation errors never cross JNI.
- Existing `#[jni_fn]` class/method strings remain stable until the replacement
  adapter and rollback path are verified.

## Versioned FFI Ownership

`eh_ffi` owns the versioned response-parser and request/plan exports backed by
`eh_domain`/`eh_net`, archive writer/validator exports backed by `eh_archive`, the
unified V2 archive-reader exports, one always-available Rust Core health export,
and the stable image/WebP exports. Its `webp` module owns decoder handles and
synchronization; eh_image::webp owns encoded input, Rust single-frame decoding,
premultiplied animation composition and resource policy;
`jvm::{bitmap,hardware_buffer,webp}` owns Android/JVM validation and storage access.
The crate also owns the direct-buffer/UTF-8/CBOR/panic-containment primitives used by
the Rust JNI adapters. The root packaging facade contains no parser or compatibility
adapter. `eh_ffi` has no Android lifecycle, credential, path, persistence,
legacy-parser, or unrelated business-decision ownership.

The root static library directly depends on `eh_ffi` and forwards its `jvm`, `android`,
and `android-26` features.
CMake builds a behavior-free link anchor, forces unresolved `Java_*` symbols out of
the Rust archive, and exports them from the final shared library. CI builds the
archive and final x86_64 library, extracts symbols with `nm`, and compares the exact
78-symbol V1, 13-symbol Archive V2, nine-symbol Android image, three-symbol host WebP,
single Hash compatibility, and single `JNI_OnLoad` sets with checked-in manifests.
Negative manifests reject the fourteen retired unversioned parser exports, three
retired GIF exports, nine old C Archive JNI exports, and `strnatcmp`; moving or
reintroducing an implementation therefore cannot silently drift ownership or ABI.
libwebp-sys2 and C webp/webpdemux build/link dependencies are removed. CMake now
links only the behavior-free anchor, Rust core and Android system libraries.
Pinned Rust codec patches and licenses live under app/src/main/rust/vendor.

Host unit coverage measures the pure codec rather than pretending JVM object access
is business logic. The codec validates positive bounded lengths, UTF-8 and CBOR,
checked result lengths, atomic destination writes, and currently has 100% line and
function coverage. The Rust Core health owner independently has 100% line (74/74)
and function (5/5) coverage. The host-testable WebP owner independently has 99.63% line and
100% function coverage. JVM/Android object glue remains covered by Android-target
compilation, Kotlin contracts, exact symbol retention, and instrumentation.

## Rust Core Health Boundary

`eh_ffi::health` is an always-testable transport diagnostic, not a rollout feature.
Its V1 request is exactly `{ version: u8, nonce: i64 }`. A supported request returns
an `ApiEnvelope` whose `module` is `rustCore`, whose `version` and payload
`protocolVersion` are both `1`, and whose payload echoes the complete signed nonce.
An unsupported request version returns the fixed typed error
`unsupported Rust Core health request version` without echoing request data.

`RustBridge` exposes `isReady()`, `checkVersion(expectedVersion)`, and `ping(nonce)`.
`isReady()` performs one fixed-nonce probe and converts `Exception` or `LinkageError`
to `false`; `checkVersion` and `ping` are strict and surface boundary failures. Fatal
`Error` values always propagate. The single
`Java_com_hippo_ehviewer_bridge_RustBridgeKt_nativeRustCoreHealthV1` adapter uses the
shared checked direct-buffer decoder and atomic response-buffer encoder.

This boundary is deliberately not called from startup, `EhApplication`, or
`JNI_OnLoad`; it creates no setting, log, credential flow, Android side effect, or
rollout decision. File-level rollback removes the health owner, JNI adapter, Kotlin
bridge/tests, CI gate, and one manifest line, restoring the prior 42-symbol ABI.

## Rust Mainline Boundary

Rust is the sole production core for portable behavior. Android persists and exposes
no backend setting, and production contains no module selector, master selector,
legacy shadow computation, parity comparison, exception/linkage fallback, effect
replay, or mid-operation backend switch. Retired preference names exist only in
startup cleanup that deletes stale upgrade data without consulting their values.

Every complete operation creates one Rust state machine, request plan, parser call,
or native handle and retains it for the operation lifetime. Boundary and domain
failures remain observable through the existing typed error surface. Operational
recovery retries or reports the operation; backend rollback requires a complete
application-version rollback.

Room, DataStore, Paging, WorkManager, notifications, Compose, SAF, platform HTTP,
and CookieManager remain Android-owned. Rust owns portable parsing, planning,
validation, filtering/sorting and state transitions. Runtime parity lives only in
frozen characterization and shared Kotlin/Rust fixtures.

The former 32-module and single-master rollout contracts are historical evidence,
not current architecture. See [ADR 0007](adr/0007-single-rust-core-selection.md).

Any later architecture paragraph that names `useRustCore*`, `default-off`, global
off/on, legacy-first parity, or pre-effect fallback records a pre-mainline slice only.
The production implementation follows the direct Rust contract above.

## Remaining Portable Rules

Filename compatibility and batched text-range normalization now invoke direct Rust
owners. UTF-16 strings, allocation limits, independent coverage, shared wire goldens
and native-library retirement stages are tracked in [P3 migration](p3-migration.md).
This does not move platform effects, HTTP, Room, Compose or third-party C codecs.

## Session Failure Policy Boundary

`eh_domain::session` owns only the pure precedence used after an Android HTTP or
parser failure: Cloudflare challenge, empty-body IP ban, gallery-unavailable,
HTTP status, remaining typed domain errors, parse handling, then rethrow. Android
sends a sanitized V1 snapshot containing booleans, host category, failure source,
and an optional `RustError`; raw URLs, Cookie/Set-Cookie headers, response bodies,
passwords, `ipb_pass_hash`, `igneous`, `cf_clearance`, API keys, and WebView data
must never cross this boundary.

Android remains the sole owner of Ktor/Cronet/OkHttp, `CookieManager`, WebView
login, credential persistence, localized strings, parse-error files, and the
existing exception hierarchy. Coroutine cancellation never enters JNI. The
Android invokes the Rust classifier before any exception or parse-file side effect.
JNI/CBOR/plan validation failure surfaces without running a second business mapper;
a validated decision is applied exactly once.

## Session Lifecycle Boundary

`eh_domain::session::lifecycle` owns the pure mode transition shared by startup,
password and WebView authentication, guest entry, and logout. Its independent V1
`sessionLifecycle` request carries only identity-cookie presence, persisted session
booleans, an optional username, a public/restricted site category, and one command.
It returns a canonical `SessionLifecycleState`, site, awaited-cookie-removal fact,
and profile update. Credential text, cookie names/values, headers, URLs, response
bodies, API keys, and WebView data never cross JNI.

The lifecycle type is intentionally separate from the established account-capability
`SessionState` DTO. Restore gives `needSignIn` safety precedence, authenticates only
when both the persisted signed-in fact and identity cookies agree, and otherwise
normalizes to sign-in-required or guest mode. A completed authentication without
identity cookies returns typed `NeedLogin` rather than a forged success.

Android owns one `SessionController` with a coroutine mutex and generation token.
Cookie removal is suspending and completes before a login request or state publication.
An older password/WebView/profile result cannot finish after a newer logout or login
attempt. `postLogin` uses structured concurrency, the WebView handoff is one-shot,
and startup restore completes before the splash gate releases. Rust lifecycle plans
are validated before application; typed business errors and partial application are
never replayed through another implementation.
See [ADR 0003](adr/0003-session-lifecycle.md) for the complete transition table.

## Gallery Catalog URL Boundaries

`eh_net::gallery::list_url` owns the pure remote request shape for all seven
`ListUrlBuilder` modes: normal, uploader, tag, popular, image search, watched
subscriptions, and toplist. Its V1 request contains the selected public/restricted
site, authentication fact, pagination/filter fields, and an already selected
`language:*` tag. Rust owns category inversion, guest `NON_H` correction, language
prefix rules, host/path selection, percent-encoding, and exact query insertion
order. Cookies, response bodies, gallery access tokens, credentials, and Android
objects never enter this protocol.

Category taxonomy is canonical in `eh_domain::gallery_rules`: the ten selectable
bits, private/unknown sentinels, historical aliases, canonical labels, and masked
inversion are shared by Gallery List/detail/metadata parsing and both catalog URL
directions. Kotlin keeps only numeric presentation constants, category labels/colors,
settings, and Compose state; category parsing and remote-mask inversion do not have
a second production implementation.

Kotlin retains the mutable `@Serializable ListUrlBuilder`, navigation/SavedState,
settings, language-index table, and HTTP execution. During the default-off rollout,
`build()` first constructs the unchanged legacy URL, then invokes Rust and accepts
the returned URL only after byte-for-byte equality with that legacy target. JNI,
CBOR, envelope, linkage, or equality failure occurs before the URL is used and
returns the legacy result. Secret-bearing `EhUrl` APIs and removal of the parity
comparison remain later migration slices.

`eh_net::gallery::list_url_parser` owns classification of an already sanitized
incoming catalog URL: root/image-search, uploader, tag, toplist, and category
aliases plus the exact cursor, category, and advanced-search defaults. Android
retains Ktor URL syntax parsing and the existing exact host allowlist, then sends
only the first two meaningful decoded path segments and the first values of the 18
relevant query fields. The original URL, ignored path suffixes, unrelated query
fields, cookies, credentials, and response bodies never cross JNI. The default-off
adapter first runs the unchanged parser and adopts Rust state only after complete
`ListUrlBuilder` equality; any boundary, linkage, or parity failure returns that
legacy state before navigation.

`eh_net::gallery::favorites_url` is a separate V1 protocol because remote favorite
slots have different sentinel and paging rules. It maps `-1` to `favcat=all`, maps
`0..=9` to numbered remote slots, omits every other category, and preserves the
legacy `favcat, f_search, prev, next, seek` order. Kotlin retains local favorites,
Room queries, the `-2` local sentinel, mutable `FavListUrlBuilder`, and remote HTTP
execution. Local lists never invoke JNI. Remote URL construction is default-off
and uses the same legacy-first, exact-equality rollback rule. Favorite mutation
URLs containing gallery IDs/tokens remain outside this read-only list slice.

## Gallery Navigation URL Boundary

`eh_net::gallery::navigation` owns the two URL grammars that enter gallery detail
and the single-page reader. One versioned `galleryNavigation` request selects
`detail` or `page`, carries the candidate string, and records whether the frozen
parser used strict host/path matching or loose embedded-reference matching. The
result is a closed `none/detail/page` plan containing only a signed gallery ID, a
ten-character lowercase hexadecimal token, and, for page matches, a zero-based
page index. Rust accepts at most 16 KiB of UTF-8 input; candidate strings and tokens
never enter errors or logs.

Compatibility deliberately preserves the existing case-sensitive `Regex.find`
behavior, including public/restricted hosts, public `/lofi`, detail `/g` and `/mpv`,
reader `/s`, strict-prefix substring matching, and the loose detail-token boundary.
Numeric overflow now produces no match instead of an uncaught Kotlin conversion
exception. Android still owns deep-link dispatch, destination construction, and
navigation side effects.

`useRustCoreGalleryNavigation` defaults to `false`. Both compatibility wrappers run
the side-effect-free legacy grammar first; unsupported inputs never cross JNI. For
a legacy match, Android adopts Rust only after version/module/shape validation and
exact plan equality. Boundary, linkage, length, or parity failure returns the
already parsed legacy result before navigation. The real API-35 MVP contract covers
one detail and one page reference through JNI. A second deterministic device test
checks package-scoped Android intent-filter resolution for both gallery hosts and
both HTTP schemes, rejects an unknown host and FTP, then launches `MainActivity`
through an implicit page-zero `ACTION_VIEW`. It follows the real intent channel and
Compose navigator and asserts
the `ProgressScreen` sentinel state before HTTP. System-wide verified-domain default
routing, clipboard dispatch, and positive-page network navigation remain separate
gates. See
[ADR 0005](adr/0005-gallery-navigation.md).

## Gallery Favorites Response Boundary

`eh_net::gallery::favorites` separately owns the remote Favorites HTML response.
Its V1 `galleryFavorites` envelope returns `GalleryFavoritesPage`: ten paired
category names and signed 32-bit counts followed by the shared `GalleryPage`
contract. The parser reuses `eh_net::gallery::list` for the gallery rows, cursors,
and typed `NoHits`, `NoWatched`, warning, and non-HTML/IP-ban behavior.

Category compatibility remains intentionally exact. `.fp` rows are visited in
source order; ordinal index 10 is skipped before parsing. Every other row reads the
same direct child positions, parses the count as strict untrimmed `i32`, XML-decodes
and trims the name, and drops that row on missing children, overflow, or invalid
entities. Empty decoded names are valid. The surviving panel must contain exactly
ten pairs before Gallery List parsing begins; otherwise Rust returns the fixed,
body-free `Gallery favorites response is invalid` error. Safe child access replaces
the legacy parser's possible index panic without changing successful output.

Android retains both GET and completed mutation HTTP requests, cookies, local
favorites and the `-2` sentinel, paging state, Room enrichment, mutation inputs,
refresh, and UI. `useRustCoreGalleryFavorites` defaults to `false` and selects one
complete parser for the already executed response. A selected V1 linkage, UTF-8,
CBOR, envelope, parser, or adapter failure is surfaced; it never replays the request
or feeds the consumed buffer to the legacy parser.

## Gallery Token Response Boundary

`eh_net::gallery::token` owns only the JSON response grammar of the remote
`gtoken` API. It decodes the complete `tokenlist` before returning the first
entry through the stable `eh_domain::GalleryToken` payload and `galleryToken`
envelope. Every item must contain a signed 64-bit `gid` and string `token`; the
response gid is intentionally ignored because the frozen Kotlin parser neither
returns it nor compares it with the request. Unknown fields are ignored, while
empty, opaque, whitespace, and Unicode token strings remain valid.

Valid fallback objects, including empty objects and null service messages, map to
the fixed `Gallery token request failed` error. Malformed JSON, non-object roots,
or invalid fallback-field types map to `Gallery token response is invalid`. Raw
response text, service messages, and tokens never enter errors or logs.

Android retains the request gid, existing gallery token, one-based page encoding,
site selection, HTTP session, referer/origin, and navigation. The default-off
`useRustCoreGalleryToken` switch selects text legacy or direct-buffer V1 parsing
before executing the single request. A selected V1 linkage, UTF-8, CBOR, envelope,
parser, or adapter failure is surfaced and never replays the request through the
legacy parser.

## Gallery Tag Vote Response Boundary

`eh_net::gallery::tag_vote` owns only the JSON response grammar of the remote
`taggallery` API. A string `error` field wins even when `tagpane` is present and is
preserved because the service uses it for user-facing tag correction suggestions.
A missing or non-string error is ignored. Otherwise `tagpane` must be a string;
malformed JSON, a missing pane, or invalid tag markup maps to the fixed
`Gallery tag vote response is invalid` failure without copying the response body or
pane into diagnostics. Plain text and fragments without tag rows remain successful
empty results.

`eh_net::gallery::tags` owns the shared row grammar used by Gallery Detail and Tag
Vote while keeping their historical differences explicit. Both preserve group/tag
source order, strip the namespace colon, decode supported entities, retain text
before the first alias separator, and map `gt/gtl/gtw` plus `tup/tdn` classes. Detail
requires non-empty text but permits a missing vote child; Tag Vote permits empty text
but requires the first element child, with unknown marker classes mapped to no vote.
Kotlin maps the raw namespace through the existing closed `TagNamespace` set.

The stable V1 payload is `GalleryTags` in the `galleryTagVote` envelope. Android
retains API credentials, gallery id/token, submitted tag text and vote, HTTP/session
state, and UI replacement. `useRustCoreGalleryTagVote` defaults to `false` and selects
the unversioned root Rust parser or workspace V1 parser before the one request is
executed. A selected V1 failure is surfaced and never replays the request through the
legacy JNI symbol.

## Gallery Rate Response Boundary

`eh_net::gallery::rate` owns only the JSON response grammar of the remote
`rategallery` API. The stable `GalleryRating` payload carries `rating` as a
32-bit float and `ratingCount` as a signed 32-bit integer in the `galleryRate`
envelope. Unknown fields, including an unrelated `error`, are ignored, and
duplicate required fields use their last JSON value to match the frozen Kotlin
serializer. No rating-range or non-negative-count policy is introduced.

`rating_avg` accepts a JSON number only when narrowing it to `f32` remains
finite; `rating_cnt` must fit the complete signed `i32` range. Missing, null,
wrongly typed, malformed, non-object, overflowed, or non-finite results map to
the fixed `Gallery rating response is invalid` failure. The response body and
remote fields never enter diagnostics.

Android retains API credentials, gallery id/token, the existing half-step
rounding of the submitted rating, HTTP/session state, and detail-screen
mutation. `useRustCoreGalleryRate` defaults to `false` and selects text legacy
or direct-buffer V1 parsing after constructing one request. A selected V1
linkage, CBOR, envelope, parser, or adapter failure is surfaced without
replaying the rating POST through the legacy parser.

## Gallery Metadata Response Boundary

`eh_net::gallery::metadata` owns only parsing and normalization of the remote
`gdata` JSON response. Its `galleryMetadata` envelope carries a source-ordered
`GalleryMetadataBatch`; each item contains the response gallery ID, normalized
titles/category/thumbnail/uploader, the original Unix timestamp in seconds,
finite `f32` rating, simple tags, and signed 32-bit page count.

Compatibility intentionally preserves the shared Kotlin JSON decoder. Unknown
fields are ignored, final duplicate JSON fields win, a missing or null uploader
becomes absent, the whole response validates before mutation, and duplicate
response IDs remain in source order. Entity decoding is a single pass limited to
`amp`, `lt`, `gt`, `quot`, `#039`, `nbsp`, and `times`; unknown entities and
nested results remain untouched. Category matching is ASCII case-insensitive but
does not trim. Thumbnail normalization applies the legacy three full prefixes in
order; it deliberately preserves `t/` after `ehgt.org/` and on relative keys.

Android retains request construction, gallery tokens, 25-item batching, five-way
group concurrency, response-to-first-object matching, date presentation,
language derivation, and list mutation. `useRustCoreGalleryMetadata` defaults to
`false` and selects exactly one response representation for the already-created
request. A selected V1 linkage, CBOR, envelope, parser, or adapter failure is
surfaced without replaying the request through the legacy parser.

## Gallery Comment Vote Response Boundary

`eh_net::gallery::comment_vote` owns only the JSON response grammar of the
remote `votecomment` API. The stable `GalleryCommentVote` payload carries the
returned comment `id` as signed 64-bit plus `score` and `vote` as signed 32-bit
integers in the `galleryCommentVote` envelope. Unknown fields, including an
unrelated `error`, are ignored; duplicate required fields use their final JSON
value. No positivity, vote-direction, score, or request-identity rule is added.

Missing, null, wrongly typed, malformed, non-object, or overflowing required
fields map to the fixed `Gallery comment vote response is invalid` failure. The
response body and remote fields never enter diagnostics.

Android retains API credentials, gallery/comment identifiers, the submitted
vote, HTTP/session state, comment refresh, and UI messaging. The independent
`useRustCoreGalleryCommentVote` switch defaults to `false` and selects text
legacy or direct-buffer V1 parsing after constructing one request. A selected
V1 linkage, CBOR, envelope, parser, or adapter failure is surfaced without
replaying the mutation POST through the legacy parser.

## Gallery Archive Response Boundary

`eh_net::gallery::archive` owns parsing the remote archive-choice page, optional
account balances, the two established archive business failures, and the final
download target. Stable choice/funds/target DTOs live in `eh_domain`; Android maps
them to the existing archive dialog model. Android still owns site selection,
funds HTTP requests, archive POST forms, retry timing, intents, `DownloadManager`,
paths, cookies, and all downloaded bytes.

The final target may use a dynamic archive host, so it is not host-allowlisted.
Rust nevertheless requires an absolute HTTPS URL with a host, rejects credentials,
fragments, control characters, relative links, and conflicting/duplicate `start`
fields, and returns exactly one `start=1` while preserving unrelated query fields.
A missing `continue` element remains a successful pending result so Android keeps
its one-shot retry. A present malformed or unsafe link is a typed failure and must
not run the legacy string-concatenation parser afterward.

`useRustCoreGalleryArchive` defaults to `false` and selects either the complete V1
list/target path or the frozen unversioned parser before the response buffer is
consumed. This is a module rollback switch, not per-response fallback: once V1 is
selected, envelope, parser, or security failures are surfaced without replay.

## Image Limits Response Boundary

`eh_net::home` owns only the pure parsing of the account home-page image quota.
Its stable `eh_domain::ImageLimits` payload contains `current`, `maximum`, and
`resetCost`; Android keeps the home/funds HTTP requests, their parallel composition,
the reset POST, torrent-key parsing, localized UI, cookies, and credentials.

Compatibility is exact: comma-stripped `strong` values that fail signed 32-bit
parsing are skipped. Exactly three remaining values map in source order, including
negative values. Exactly one remaining value ignores that number and emits
`current=0`, `resetCost=0`, and `maximum=0` only when the panel contains the
case-sensitive text `No restrictions`; otherwise `maximum=-1`. Every other count
is a typed parse failure.

`useRustCoreImageLimits` defaults to `false` and selects V1 or the frozen parser
before the direct response buffer is consumed. A selected V1 boundary, envelope,
or parse failure is surfaced and never replays the same body through legacy code.

## News Event Pane Response Boundary

`eh_net::news` owns only the `#eventpane` grammar in the News HTML response. Its
stable `eh_domain::NewsPage` payload contains nullable `eventHtml`: a missing pane
is absent, an existing empty pane remains an empty string, and nested markup plus
entity text preserve the DOM parser's inner-HTML representation. Gallery Detail
reuses the same crate-internal extractor instead of maintaining a second grammar.

Android retains the News HTTP request, Cookie star side effect, daily scheduling,
notification delivery, and the `parse=false` request-only path. The independent
`useRustCoreNews` switch defaults to `false` and is intentionally outside the
twenty-one-setting MVP rollout. `EventPaneParser` selects either the frozen parser
or V1 before consuming the direct buffer; cancellation and typed failures propagate,
and a selected V1 failure never replays the body through legacy code.

## User Configuration Boundary

`eh_net::user_config` owns only favorite-category name extraction from the remote
user-configuration HTML. Its stable `eh_domain::FavoriteCategories` payload keeps
successfully decoded names in source order. Android retains both site URLs, HTTP,
the restricted-site redirect retry, cookies, credentials, preference mutation,
and the final ten-category settings invariant.

Compatibility intentionally remains narrow: a match requires the exact
double-quoted `type="text" name="favorite_<digit>" value="..."` attribute order,
one ASCII digit, and a non-empty value. Duplicate indexes, whitespace, and source
order are preserved. Supported XML plus `nbsp`/`times` entities are decoded; a
value with an invalid entity is dropped independently. Empty or partial lists are
successful parser payloads and are rejected only if Android attempts to apply them
to the fixed-size setting.

`useRustCoreUserConfig` defaults to `false` and selects exactly one parser before
the direct response buffer is consumed. V1 linkage, envelope, or adapter failures
are wrapped by the existing User Config parse boundary and never replayed through
the legacy parser.

## Torrent List Boundary

`eh_net::torrent` owns only pure parsing of gallery torrent tables. Its stable
`eh_domain::TorrentListPage` payload preserves the legacy display fields and a
validated `infoHash`; the source `onclick` URL is intentionally absent. Android
retains the torrent endpoint and referer, HTTP, gallery/tracker key lookup, magnet
construction, `Intent`/clipboard behavior, localization, and UI.

Compatibility preserves source order and the legacy per-table skip policy:
case-sensitive `Expunged` tables, rows that miss the frozen field grammar, numeric
overflows, and names with invalid entities are omitted independently. A remaining
row is exposed only when the final slash-delimited URL segment is exactly 40 ASCII
hex characters followed by lowercase `.torrent`; uppercase hashes are normalized
to lowercase. The prefix may be relative or absolute and is discarded instead of
inventing an unsupported host allowlist. Queries, fragments, suffix changes, and
invalid hash lengths or characters cause only that row to be skipped.

`useRustCoreTorrent` defaults to `false` and selects one complete parser before the
direct response buffer is consumed. Only the frozen legacy result may derive a
hash from its raw URL; V1 linkage, CBOR, envelope, parser, or adapter failures are
surfaced without replay. Copying the complete magnet (including the tracker key)
remains an existing Android-owned fallback and requires separate device/security
evidence before any behavior change.

## Reader MPV Token Boundary

`eh_net::reader::mpv` owns only extraction of the first case-sensitive
`var imagelist = ` assignment from gallery multi-page-viewer HTML and strict JSON
decoding of each item's `k` field. Its stable `eh_domain::MpvTokens` payload keeps
page-index order, duplicates, empty strings, Unicode escapes, and values with
unknown sibling fields. Rust deliberately applies no unevidenced token format or
length rule. Missing boundaries, a non-array, malformed JSON, or any missing,
null, or non-string `k` field fails the complete response with a sanitized error
that contains neither HTML nor tokens.

V1 requires the marker and thereby closes the frozen Kotlin parser's accidental
markerless fixed-offset substring path; the default-off legacy branch remains
available if this tightening causes a compatibility issue. Android retains the
MPV URL/referer, HTTP and cookies, `SpiderInfo.pTokenMap`, list-index-to-page
mapping, request serialization, page URL/API construction, and the existing
cached-token → MPV → preview → preview-retry fallback.

`useRustCoreReaderMpv` defaults to `false`. `EhEngine` chooses text legacy or
direct-buffer V1 before executing the single HTTP request. A V1 linkage, UTF-8,
CBOR, envelope, parser, or adapter failure is surfaced to the existing reader
fallback; it never re-executes the MPV request through the legacy parser.

## Android Hardware-Buffer Boundary

Android keeps ownership of Bitmap and HardwareBuffer objects while `eh_image` owns
the host-testable crop algorithm and `eh_ffi::jvm::hardware_buffer` owns JNI storage
validation. The adapter accepts only exact packed-format
pairs, validates layer/usage/stride/address ranges, and creates bounded disjoint
slices while both native allocations are held by RAII lock guards. Repeated
device calls are specified to verify that successful locks are released. Once the
image-device CI run succeeds, its API 29+ instrumentation also provides evidence
that the wrapped Bitmap retains the native buffer after Kotlin closes its
HardwareBuffer handle.

## Animated WebP FFI Boundary

`eh_image::webp` owns the pure-Rust decoder/compositor and bounded input/raster
lifecycle; `eh_ffi::webp` owns the synchronized decoder registry;
`eh_ffi::jvm::{webp,bitmap}` owns the unchanged JVM and Android JNI adapters. The
root compatibility crate only links those exports into its static archive. The
boundary follows this ownership contract:

- Kotlin passes only the remaining `ByteBuffer` slice; Rust immediately copies
  those encoded bytes and retains them for the decoder lifetime.
- JNI exposes a positive opaque handle backed by a synchronized Rust registry,
  never a native pointer encoded as `Long`.
- Rust serializes metadata, decode, reset, and destroy operations per registry;
  destroy is idempotent and cannot race a frame callback.
- Kotlin owns exactly two RGBA frame bitmaps. Disposal cancels its decode scope;
  synchronized native destroy waits behind any active frame callback before the
  bitmaps are recycled. Rust validates format, dimensions, stride, and address
  arithmetic before copying a decoded canvas.

Host tests decode the checked-in ten-frame fixture, cover input ownership, callback
failure, independent handles, metadata limits, reset, invalid input/handles, and
registry poisoning. P3 additionally compares full premultiplied RGBA canvases with
pinned libwebp 1.6.0 lossless/lossy/alpha/blend/dispose oracles and frame hashes.
Bounds and codec allocation hooks are described in p3-migration.md. API-35 instrumentation constructs and draws the real animation,
checks remaining-buffer preservation, start/stop, malformed input, and idempotent
disposal. Exact host and Android symbol manifests prevent this source move from
changing any JNI name.

## Download Queue Boundary

`eh_domain::download` owns the platform-independent single-worker queue state
machine. Android sends a complete ordered snapshot plus one V1 command and gets
back the final snapshot, exact state changes, and optional `startGid`/`cancelGid`
effects. Rust owns positive/unique identifier validation, FIFO invariants,
start/stop/finish transitions, and process-restart normalization. No destination
path, credential, database handle, or Android object crosses this contract.

Android remains the sole owner of Room rows, `SpiderQueen`, speed tracking,
foreground-service notifications, files, and archive shortcuts. The adapter
serializes commands under the fixed `sortMutex -> mutex` lock order, applies one
validated plan, then executes those platform effects. Orphan persisted
`waiting`/`downloading` states are normalized through the Rust recovery entry
point without clearing a valid live worker or FIFO. A default-off
`useRustCoreDownloadQueue` setting retains the legacy scheduler as the immediate
rollback path; a failed Rust boundary never retries the same command through
legacy code after partial mutation.

## Download Progress Boundary

`eh_domain::download::progress` is a dedicated pure owner for one active task's
immutable reset, page-total, page-result, ETA, and terminal-state planning. Its V1
request/response contract uses exact CBOR with command properties ordered as
`kind`, `finished`, `downloaded`, `total`, `speedBytesPerSecond`, `transfers`, and
`failed`. Rust owns signed JVM-compatible wrapping and division; Android continues
to own workers, transfer maps, `SpeedTracker`, Room, `SpiderQueen`, listeners,
files, and every side effect.

Each callback captures the expected `DownloadInfo` when it is received. Enabled
mode then acquires `sortMutex -> mutex`, verifies object identity with the current
task, and acquires the transfer mutex last only when a transfer snapshot is needed.
No queue mutex is acquired while the transfer mutex is held. A stale callback is
ignored before mutation and emits only a fixed, identifier-free diagnostic. Reset
and Finish remain synchronized under the queue locks, including the final
queue/progress consistency check before any cleanup, persistence, listener, or
worker-advance effect.

`useRustCoreDownloadQueue` remains default-off. Disabled progress callbacks retain
the legacy lock-free queue path; enabled selection builds the full legacy plan
first, calls its V1 JNI boundary once, and adopts Rust only after exact plan
equality. `Exception` and `LinkageError` fall back before effects, cancellation and
fatal `Error` propagate, and no applied command is replayed.

## Reader Scheduling Boundary

`eh_domain::reader` owns the pure request-direction, cache-hit, retry, restart,
and prefetch decisions used by `PageLoader`. Android sends the ordered page-state
snapshot, previous requested index, cache-hit fact, prefetch distance, and one V1
command. Rust validates a maximum of 100,000 pages and a prefetch distance of at
most 1,024, then returns ordered actions such as `setPrevious`, `useCached`,
`fetch`, and `prefetch`. Empty legacy progressions still produce a prefetch action
with their original bounds.

Android remains the sole owner of `Image`, Compose flows, cache storage and locks,
`SpiderQueen`, archive sources, decoding jobs, and network effects. The Kotlin
adapter validates the complete command-specific action order before mutation and
applies it exactly once. Scheduling calls are serialized; a cached image is
temporarily pinned across JNI so eviction cannot recycle it during planning.
`useRustCoreReaderSchedule` defaults to `false`. JNI/CBOR/plan validation failure
may use the unchanged legacy method only before the first action; an action
application failure is surfaced and is never replayed through legacy code.

## Reader Fetch Policy Boundary

`eh_domain::reader::fetch` owns the pure transition policy for one image-fetch
operation from cache selection through token acquisition, bounded HTML/API
attempts, original/resized selection, download outcome, outer-failure
classification, and terminal publication. Its `readerFetch` V1 state/event/action
schema contains only closed control facts. Page/gallery identities, URLs, tokens,
paths, bodies, cookies, exceptions, display strings, credentials, and Android
objects never enter Rust or its diagnostics.

`SpiderQueen` retains every effect: cache and temporary files, pToken/show-key
storage, locks and FIFO throttling, HTTP, URL construction, `SpiderDen`, page
state, listeners, localized errors, and cancellation. The implementation is
selected and its `Start` plan validated before the first effect. Ordinary
boundary/linkage/parity failure may select legacy only at that initial gate;
after Rust is accepted, all failures propagate without replay or switching.

Cache bootstrap plus current/previous pToken acquisition completes under
`pTokenLock` and returns the still-acquiring reducer state. Android snapshots
show-key availability under `showKeyLock` only after releasing the token lock.
On image failure, Android removes intermediates and emits its fixed diagnostic
before the later JNI transition. It then validates and consumes exactly one
leading cleanup action from the returned plan, preserving every remaining
action. Cancellation cleans and rethrows; `FileNotFoundException` cleans then
enters outer classification; ordinary API failure retries through HTML without
replacing the retained display error.

`useRustCoreReaderFetch` is independent and defaults to `false`. It is excluded
from the coordinated twenty-one-setting MVP profile, which remains unchanged.
The exact host V1 ABI is 44 symbols. The isolated production gate is 201/201
lines and 7/7 functions and writes `eh-domain-reader-fetch.lcov`. Focused JVM
contracts pass 21/21 and the real-JNI test source compiles, but API 35 execution
is UNRUN; no device, rollout, or default-change claim is made. See
[ADR 0006](adr/0006-reader-fetch-policy.md).

## Archive Completion Validation Boundary

`eh_archive` validates the exact CBZ shape emitted by the application without
depending on Android or filesystem paths. Generated archives must contain
sequential non-empty page entries followed by one `ComicInfo.xml`; all entries
must be regular, unencrypted Stored files. The validator reads every entry to EOF
so CRC corruption is observed before completion, while rejecting compressed
entries before payload reads prevents decompression amplification.

Android retains path/SAF ownership, page files, cleanup, and
download callbacks. The V1 JNI adapter in `eh_ffi::jvm::archive` duplicates the
Android-owned descriptor, seeks the duplicate from zero, restores its shared seek
offset, and closes only the duplicate. `SpiderDen`
invokes validation after closing the writer and before `SpiderQueen` announces
success. Failure deletes the generated CBZ but keeps source pages; cancellation
performs the same cleanup and is rethrown. Validation always uses the Rust adapter
and has no runtime selector or replay path.

## Generated CBZ Writer Boundary

`eh_archive::writer` owns deterministic generation of the application's exact
CBZ shape. It accepts only ordered `Read` inputs, one sequential page name per
expected image, a final `ComicInfo.xml`, and a `Write` output. Counts and names
are validated before output begins. Every entry must then produce at least one
byte and is emitted with Stored compression and Unix mode `0644`; ZIP metadata
does not copy platform UID, GID, or timestamps. `ZipWriter::new_stream` keeps the
core compatible with sequential outputs and prevents Android seek behavior from
leaking into the host-only crate.

Android retains page/cache sources, ComicInfo generation, paths/SAF, descriptor
lifetime, download completion, and cleanup. Kotlin opens fresh source descriptors
and one truncate-mode `rwt` output descriptor, then calls the Rust V1 writer. The JNI
adapter in `eh_ffi::jvm::archive` duplicates every descriptor and closes only the
duplicates; matching the established contract, sources are consumed from their
current position to EOF. The old `archiveFdBatch` declaration and C implementation
are absent from production.

Once Rust writing starts, any input, output, empty-entry, overflow, or ZIP
finalization failure is surfaced and never falls through to the legacy writer on
the potentially partial output. The adapter attempts to truncate a failed
output, while `SpiderDen` and export callers remain the authoritative owners that
discard incomplete files and preserve source pages. Validation follows the same
captured backend; it does not introduce another selection point.

## Unified Multi-Format Archive Boundary

`eh_ffi::jvm::archive::archive_native` owns the unified V2 reader lifecycle and
uses format-specific codecs: ordinary ZIP/CBZ uses eh_archive::production_zip,
RAR/RAR5 uses rars, and 7z uses sevenz-rust2. TAR uses production_tar; outer
gzip/xz uses archive_filters with Rust flate2/lzma-rust2 codecs. libarchive,
liblzma, nettle and their CMake fetch/build/link steps are removed. The Rust
boundary detects formats by content and supports the encrypted-ZIP password
flow, retains the eleven lowercase playable extensions and natural ordering, and
enforces the 100,000-entry and 512-MiB-image limits. Unsupported codec encryption
or compression is an explicit Rust failure, never a reason to select legacy.

Open scans and indexes playable metadata in Rust. Each extraction reopens a bounded
view from a duplicated source descriptor, restores the shared source offset, and
writes only to a Java-owned direct buffer or duplicated output descriptor. Positive
opaque handles are synchronized; close is idempotent and waits for active extraction.
No native pointer, path, Android object, or Kotlin replay of archive logic crosses
the boundary.

`ArchivePageLoader` sends every archive to Rust V2 regardless of filename extension
and surfaces open, password, index, or extraction failures. The old C reader, Kotlin
JNI declarations, natsort implementation, and private native header are absent from
production, including the former libarchive codec adapter. Extended/sparse TAR,
compressed ZIP, shared decoder heap bounds and bounded replay are documented in
p3-migration.md; no C fallback or runtime backend selection remains.
Kotlin/Android owns SAF, descriptor acquisition, password UI, and page lifecycle.
API-35 and physical/SAF execution remain separate device evidence.

## GIF Delay Workaround Boundary

`eh_image::gif` owns exact `GIF87a`/`GIF89a` recognition and the historical Android
9–13 short-frame-delay rewrite. Its safe bounded scan preserves the prior extension
marker, terminator, and first-normal-delay stop rules without the C size underflow.

`eh_ffi::jvm::gif::prepareGifSource` duplicates the Android descriptor through the
shared checked helper, uses offset-based reads so the caller position does not move,
reads only six bytes for non-GIF input, and snapshots GIF input into at most 512 MiB
of Rust-owned bytes before returning a JVM-owned byte array. Non-seekable descriptors
return an unclassified null without being consumed, so Kotlin hands the path back to
Coil's normal opener instead of blocking non-GIF platform sources. Snapshot length,
tail, header, and available modification time are rechecked to contain ordinary
concurrent file changes; this is best-effort consistency rather than a filesystem
transaction. Direct buffers use the same pure rewrite synchronously. Kotlin owns only
API 28–33 selection, descriptor and Coil source lifetime, and presentation. Prepared
bytes bypass the direct-buffer path, so they are not rewritten twice. No mmap pointer
or native allocation lifetime crosses JNI; `gifutils.c`, `mmap`, and `munmap` are absent
from production.

## Gallery Title Rule Boundary

`eh_domain::gallery_rules` also owns the historical similar-gallery title cleanup.
It preserves ASCII Java `\s`, delimiter prefix/suffix stripping, the case-insensitive
`ch.` wildcard and ASCII digit grammar, Java terminal-line behavior, final-pipe
selection, Unicode trimming, and empty-to-null conversion. Android sends the nullable
title through one `galleryTitle` V1 CBOR request and uses the validated nullable plan
before constructing navigation. Boundary, linkage, or contract failure surfaces;
there is no Kotlin production fallback. Android retains title preference, localized
presentation, filename selection, and navigation effects.

## Gallery Favorites Mutation Planning

`eh_net::gallery::favorite_mutation` owns only a pure V1 plan for one single or bulk
favorite mutation: exact HTTPS URL, referer, origin, and ordered repeated form fields.
The global operation snapshot selects exactly one planner. Global on validates and
uses only Rust; global off builds only the frozen legacy plan.
HTTP, cookies, response parsing, delays, refresh, database, and UI remain Android-owned.

## Gallery Archive Request Planning

`eh_net::gallery::archive_request` owns one pure V1 request plan: the exact archive
URL, gallery-detail referer, nullable origin, and ordered direct or H@H form fields.
`GallerySite` and `FormField` live at the `eh_net::gallery` boundary because both the
archive and favorite-mutation planners use the same wire values. The separate
`eh_net::gallery::archive` module continues to own only response parsing.

The global operation snapshot selects exactly one planner before funds lookup or the
first archive HTTP request. Global on validates and uses only Rust; any boundary
failure surfaces before HTTP. Global off builds only the frozen legacy plan. HTTP,
funds, retry/delay, response parsing, download, and UI effects remain Android-owned
and are never replayed after request execution starts.

## Decision Records

Architecture changes that alter dependency direction, wire format, persistence,
or rollback behavior require a short ADR in this directory. The ADR records
context, decision, alternatives, consequences, and migration impact.
