# Test and Coverage Policy

## Critical Scope

The 95% unit-test gate applies to the Rust packages that own:

- authentication and session state;
- catalog/list request building and parsing;
- gallery detail parsing and rules;
- reader/image metadata and processing rules;
- download/archive state machines and validation;
- serialized contracts used by those features.

Each critical package must independently meet both 95% line coverage and 95%
function coverage. A workspace average cannot compensate for an under-tested
critical package.

## Test Layers

1. Unit tests are offline, deterministic, and parallel-safe. They own the 95%
   proof.
2. Contract tests use checked-in golden CBOR/JSON fixtures in Rust and Kotlin.
3. Integration tests may use HTTP test servers, JNI, a database, or Android
   framework components; they do not replace unit tests.
4. Device E2E tests cover startup, session, list, detail, reader, and download.

API 29+ image-boundary instrumentation runs on the pinned API 35 x86_64 CI
emulator. Its HardwareBuffer contract requires RGBA8888, RGB565, and RGBAF16
support; an unsupported configured format is a CI failure rather than a skipped
assertion. The same job decodes the checked-in animated WebP fixture through the
real JNI/Bitmap runtime, draws its first frame, exercises start/stop, rejects
malformed input, and disposes the decoder idempotently.

The archive acceptance lane writes generated CBZ output through real parcel-file
descriptors, validates it through the Rust boundary, inspects Stored entries, and
asserts failed output is truncated. Its unified V2 fixtures must cover ZIP/CBZ,
RAR/RAR5, 7z and tar, encrypted-ZIP wrong-password rejection and correct-password
acceptance, direct-buffer and output-descriptor extraction, source-offset restoration,
CRC/truncation failures, close races, and idempotent close. Exact current archive,
business, WebP, and image symbol manifests prevent a source move from dropping or
renaming Kotlin entry points.
Instrumentation sources must also compile in the local host gate even when no
emulator is attached.

`RustBridgeDeviceTest` calls the real Rust Core health JNI entry point through
`isReady()`, `checkVersion()`, and `ping()`, including the expected-version mismatch
and signed nonce boundaries. Its instrumentation source compiles locally, but API 35
runtime execution is UNRUN on this host and is never reported as a device pass.

`ReaderFetchDeviceTest` drives an original-image success sequence and a
`FileNotFoundException` cleanup/classification sequence through the real V1 JNI
reducer without HTTP. Its source compiles locally. API 35 runtime execution remains
UNRUN, so these two contracts are not counted as executed device tests.

`MvpFlowDeviceTest` records cross-module native-boundary coverage. The current
contract invokes the Rust business boundaries directly and surfaces boundary failures
without fallback. Fixtures remain packaged from `crates/net/test_data`.
Source compilation or APK packaging is not device execution.

`NewsDeviceTest` loads the canonical `crates/net/test_data/news.html` asset and
parses it through the real V1 JNI symbol.
Local source compilation and instrumentation-APK packaging prove the Android/JNI
contract is buildable, and the packaged `assets/news.html` is byte-identical to the
Rust fixture. It is not counted as executed until the pinned KVM-enabled job or a
physical device runs it.

`GalleryNavigationAppLinkDeviceTest` adds the adjacent Activity/Compose contract.
It first verifies package-scoped Android intent-filter resolution for public and
restricted HTTP/HTTPS gallery links and rejects an unknown host and FTP scheme. It
prepares the valid default external-files
download directory, forces the sign-in start
destination, and disables automatic update HTTP before launching `MainActivity`
with an implicit page-zero `ACTION_VIEW`. The real `onCreate` → intent channel →
`navWithUrl` path must render the tagged `ProgressScreen` and its documented
`pageIndex = -1` error; the sentinel prevents Gallery Token HTTP. Settings are
restored after the Activity closes, and a test-created download directory is removed
by an outer `finally` even if the settings snapshot fails. Together with
`MvpFlowDeviceTest`, this proves native reachability and one package-resolved Activity
navigation outcome without claiming system-wide verified-domain default routing,
clipboard dispatch, positive-page networking, or full startup-to-reader E2E.

## Local Device Execution Status

On 2026-08-03 this host attempted the API 35 x86_64 device gate with both
`google_apis` and `aosp_atd` images. The current user cannot access `/dev/kvm`, so
the emulator had to use unsupported software TCG. The AOSP ATD image reached
`sys.boot_completed=1`, but its network stack and `system_server` restarted before
APK installation. `connectedDefaultDebugAndroidTest` therefore failed at
`package install-create` with the package/activity services unavailable and
reported zero executed tests. The emulator's xTS CPU profile is not a fallback
because it requires the KVM-only `host` CPU model.

This is host-infrastructure evidence, not a pass or failure of an application
assertion. No device contract is counted as executed. A KVM-enabled emulator,
the pinned remote CI job, or a physical device remains required for the first
successful device run.

Real websites are never called from unit tests. Captured pages must be sanitized
and checked in as fixtures with their scenario and expected outcome documented.

## Rust Core Health Tests

The independent health gate covers a normal nonce plus `i64::MIN` and `i64::MAX`,
an unsupported request version with a fixed typed and request-free error, and exact
request/response CBOR goldens with round-trip decoding. Kotlin JVM tests cover one-call
readiness, strict version and nonce checks, protocol/nonce/module/payload drift,
`Exception` and `LinkageError` handling, and fatal-error propagation. The measured
health denominator is 74/74 lines and 5/5 functions; the JVM suite is 9/9.

## Favorite Mutation Planning Tests

The isolated gate covers single/bulk good-base-bad cases, signed/empty/opaque values,
source omission, ordered repeated fields, sanitized failures, and JSON/CBOR shapes.
Kotlin tests assert direct native invocation, exact Rust contract validation, and
surfaced mismatch/exception/linkage failure without a fallback callback.
API-35 instrumentation is wired to invoke the real V1 boundary offline without HTTP;
the exact manifest count is 41. Local API-35 execution currently runs zero tests and
is not counted; KVM-enabled device execution remains open.

## Gallery Archive Request Planning Tests

The isolated Rust gate covers public and restricted list/download planning, direct
and H@H form ordering, original/resampled labels, empty and opaque values, every
invalid optional-field shape, version failures, and exact JSON/request/response CBOR
goldens. Its current evidence is 5/5 tests, 237/237 lines, and 11/11 functions.
Kotlin tests cover exact Rust goldens, direct invocation, and surfaced
request/envelope/form drift without fallback. The real-JNI API-35 test source compiles
and performs no HTTP, but device execution is not counted on this host. The exact V1
manifest count is 41.

## Required Cases

Every behavior covers:

- good: normal input and expected output;
- base/edge: empty, boundary, missing optional data, first/last state;
- bad: malformed data, unsupported version, auth expiry, IO/network failure.

Every Rust-backed operation covers direct native invocation, operation-lifetime state
stability, and explicit JNI/linkage/CBOR/contract failure without fallback or replay.

Tests assert values and typed errors. `dbg!`, snapshots without reviewed
semantics, or “does not panic” alone are insufficient.

## Commands

```bash
cargo test --package eh_archive --package eh_domain --package eh_ffi --package eh_image --package eh_net
cargo test --package eh_ffi --all-features webp::tests::
cargo test --package eh_ffi --all-features health::tests::
cargo llvm-cov --package eh_ffi --lib --all-features \
  --ignore-filename-regex '(/jvm/|/jvm\.rs$|/health\.rs$|/webp\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95
cargo llvm-cov --package eh_domain --all-features \
  --ignore-filename-regex '(/download/tests\.rs$|/download/progress/tests\.rs$|/reader/fetch/tests\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/download/tests\.rs$|/download/progress/tests\.rs$|/download/progress\.rs$|/reader/|/reader\.rs$|/session/|/session\.rs$|/contracts\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- download::tests::
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/download/tests\.rs$|/download/progress/tests\.rs$|/download\.rs$|/reader/|/reader\.rs$|/session/|/session\.rs$|/contracts\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- download::progress::tests::
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/contracts\.rs$|/download/|/download\.rs$|/reader/fetch\.rs$|/reader/fetch/tests\.rs$|/session/|/session\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- reader::tests::
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/contracts\.rs$|/download/|/download\.rs$|/reader\.rs$|/reader/fetch/tests\.rs$|/session/|/session\.rs$|/lib\.rs$)' \
  --lcov --output-path target/llvm-cov/eh-domain-reader-fetch.lcov \
  --fail-under-lines 95 --fail-under-functions 95 -- reader::fetch::tests::
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/contracts\.rs$|/download/|/download\.rs$|/reader/|/reader\.rs$|/session/lifecycle\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- session::tests::
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/contracts\.rs$|/download/|/download\.rs$|/reader/|/reader\.rs$|/session\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- session::lifecycle::tests::
cargo llvm-cov --package eh_image --all-features \
  --fail-under-lines 95 --fail-under-functions 95
cargo llvm-cov --package eh_image --lib \
  --ignore-filename-regex '(/border\.rs$|/pixel\.rs$|/qr\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- region::tests::
cargo llvm-cov --package eh_net --all-features \
  --fail-under-lines 95 --fail-under-functions 95
cargo llvm-cov --package eh_archive --all-features \
  --fail-under-lines 95 --fail-under-functions 95
cargo llvm-cov --package eh_archive --lib \
  --ignore-filename-regex '(/lib\.rs$|/writer\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- reader::tests::
cargo llvm-cov --package eh_archive --lib \
  --ignore-filename-regex '(/lib\.rs$|/reader\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- writer::tests::

# Critical modules sharing a crate are also gated independently.
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/preview\.rs$|/gallery/rate\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --lcov --output-path target/llvm-cov/eh-net-gallery-favorite-mutation.lcov \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::favorite_mutation::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/reader\.rs$|/gallery/(archive|comment_vote|comments|detail|favorite_mutation|favorites|favorites_url|list|list_url|list_url_parser|metadata|navigation|preview|rate|tag_vote|tags|token)\.rs$|/gallery\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --lcov --output-path target/llvm-cov/eh-net-gallery-archive-request.lcov \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::archive_request::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/gallery/|/reader/|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- auth::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::list::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/preview\.rs$|/gallery/rate\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::metadata::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::list_url::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::list_url_parser::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/preview\.rs$|/gallery/rate\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::navigation::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::favorites_url::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::favorites::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::archive::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::comments::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::comment_vote::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::detail::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::preview::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::token::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/rate\.rs$|/gallery/preview\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::tag
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/reader/|/auth\.rs$|/gallery/archive\.rs$|/gallery/comment_vote\.rs$|/gallery/comments\.rs$|/gallery/detail\.rs$|/gallery/favorites\.rs$|/gallery/favorites_url\.rs$|/gallery/list\.rs$|/gallery/list_url\.rs$|/gallery/list_url_parser\.rs$|/gallery/metadata\.rs$|/gallery/navigation\.rs$|/gallery/preview\.rs$|/gallery/tag_vote\.rs$|/gallery/tags\.rs$|/gallery/token\.rs$|/gallery/favorite_mutation\.rs$|/gallery/archive_request\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- gallery::rate::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/gallery/|/auth\.rs$|/gallery\.rs$|/reader/mpv\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- reader::page::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/gallery/|/auth\.rs$|/gallery\.rs$|/reader/page\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/torrent\.rs$|/user_config\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- reader::mpv::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/gallery/|/reader/|/auth\.rs$|/gallery\.rs$|/reader\.rs$|/news\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- home::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/gallery/|/reader/|/auth\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/user_config\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- news::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/gallery/|/reader/|/auth\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/torrent\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- user_config::tests::
cargo llvm-cov --package eh_net --lib --all-features \
  --ignore-filename-regex '(/auth/|/gallery/|/reader/|/auth\.rs$|/gallery\.rs$|/reader\.rs$|/home\.rs$|/news\.rs$|/user_config\.rs$|/html\.rs$|/text\.rs$|/lib\.rs$|/test_support\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- torrent::tests::

# The Rust Core health owner has its own independent gate and LCOV artifact.
cargo llvm-cov --package eh_ffi --lib --all-features \
  --ignore-filename-regex '(/jvm/|/jvm\.rs$|/codec\.rs$|/webp\.rs$|/lib\.rs$)' \
  --lcov --output-path target/llvm-cov/eh-ffi-health.lcov \
  --fail-under-lines 95 --fail-under-functions 95 -- health::tests::

# The host-testable FFI WebP owner has its own independent gate.
cargo llvm-cov --package eh_ffi --lib --all-features \
  --ignore-filename-regex '(/jvm/|/jvm\.rs$|/codec\.rs$|/health\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- webp::tests::

# Requires an Android SDK/NDK and an API 29+ x86_64 device or emulator.
./gradlew :app:connectedDefaultDebugAndroidTest \
  -Pandroid.injected.build.abi=x86_64
```

As each critical crate is introduced, CI adds a separate coverage invocation
for that crate. Reports are retained as artifacts for review.

The legacy root crate is temporarily excluded from the deterministic unit-test
command because its parser suite still calls live services. Each migration slice
must replace the relevant smoke test with offline fixtures; once the final live
test is removed, `cargo test --workspace` becomes the mandatory command.

## Current Enforced Packages

The detailed counts and selector wording below are historical pre-mainline evidence.
Current tests exercise direct Rust bridges; any reference to parity, `default-off`,
rollback switches, or global backend selection is non-normative.

The current CI gate covers only critical code already migrated into the new
workspace. It does not represent completion of the full rewrite:

The counts below are the latest recorded pre-C7 evidence. They must be regenerated
after the C1-C6 single-switch and unified-archive changes before any completion claim.

| Gate scope | Critical responsibility | Local line/function evidence (2026-08-04) |
| --- | --- | --- |
| `eh_domain` | Versioned contracts, queue, progress, reader, and sanitized session policy | 99.41% (2176/2189) / 100% (116/116) |
| `eh_domain::download` | Download FIFO, recovery, state changes, and platform-effect decisions | 97.05% (329/339) / 100% (28/28) |
| `eh_domain::reader` | Request direction, cache/retry/restart actions, and prefetch planning | 100% (412/412) / 100% (22/22) |
| `eh_domain::reader::fetch` | Sanitized image-fetch transitions and ordered Android effects | 100% (201/201) / 100% (7/7) |
| `eh_domain::session` | Sanitized Cloudflare/IP-ban/HTTP/domain/parse failure precedence | 100% / 100% |
| `eh_domain::session::lifecycle` | Startup restore, login handoff, guest, and logout planning | 100% (215/215) / 100% (14/14) |
| `eh_ffi::codec` | Checked input slices, UTF-8/CBOR transport, and atomic output | 100% (77/77) / 100% (7/7) |
| `eh_ffi::health` | Versioned readiness, protocol compatibility, and signed-nonce echo | 100% (74/74) / 100% (5/5) |
| `eh_image` | Pixel normalization, border/QR detection, and safe region copying | 99.79% / 100% |
| `eh_image::region` | Crop bounds, padded row layouts, and byte-exact region copying | 100% / 100% |
| `eh_net::auth` | Sign-in and account-profile parsing | 98.42% / 96.88% |
| `eh_net::gallery::list` | Gallery List parsing and V1 envelope | 99.17% / 95.74% |
| `eh_net::gallery::list_url` | Seven-mode Gallery List request URL construction | 100% / 100% |
| `eh_net::gallery::list_url_parser` | Sanitized incoming Gallery List URL state interpretation | 99.57% / 100% |
| `eh_net::gallery::navigation` | Gallery detail and reader-page navigation URL interpretation | 98.80% (247/250) / 100% (21/21) |
| `eh_net::gallery::favorites_url` | Remote favorites-list request URL construction | 100% / 100% |
| `eh_net::gallery::favorite_mutation` | Pure single/bulk favorite mutation URL, header, and ordered/repeated form-field planning | 100% (242/242) / 100% (14/14) |
| `eh_net::gallery::archive_request` | Pure archive list/download URL, header, origin, and ordered form-field planning | 100% (237/237) / 100% (11/11) |
| `eh_net::gallery::favorites` | Favorites category panel plus shared Gallery List response and V1 envelope | 99.41% (169/170) / 100% (18/18) |
| `eh_net::gallery::archive` | Archive choices/funds and validated final HTTPS targets | 97.75% / 100% |
| `eh_net::gallery::comments` | Gallery comment parsing and V1 envelope | 99.49% / 98.11% |
| `eh_net::gallery::comment_vote` | Gallery Comment Vote JSON response and V1 envelope | 100% (103/103) / 100% (12/12) |
| `eh_net::gallery::detail` | Rich metadata, tags, embedded comments/previews, and V1 envelope | 96.88% (497/513) / 100% (46/46) |
| `eh_net::gallery::preview` | Gallery preview/page-count parsing and V1 envelope | 99.64% / 97.44% |
| `eh_net::gallery::token` | First Gallery Token API result, full-list validation, and token-free failures | 100% (96/96) / 100% (12/12) |
| `eh_net::gallery::tag_vote` + `tags` | Tag Vote JSON plus shared Detail/Vote tag-row grammar | 97.59% (283/290) / 100% (36/36) |
| `eh_net::gallery::rate` | Gallery Rate JSON response and V1 envelope | 100% (93/93) / 100% (10/10) |
| `eh_net::gallery::metadata` | Gallery List `gdata` JSON normalization and V1 envelope | 100% (218/218) / 100% (19/19) |
| `eh_net::reader::page` | Reader image URLs, retry keys, and V1 envelope | 100% / 100% |
| `eh_net::reader::mpv` | Ordered MPV page-token extraction and sanitized JSON failures | 100% (103/103) / 100% (13/13) |
| `eh_net::home` | Account image-limit parsing and legacy quota sentinels | 96.90% (125/129) / 100% (10/10) |
| `eh_net::news` | Shared News/Detail event-pane grammar and V1 envelope | 98.46% (64/65) / 100% (9/9) |
| `eh_net::user_config` | Favorite-category extraction and legacy partial-success semantics | 100% (47/47) / 100% (8/8) |
| `eh_net::torrent` | Torrent table compatibility and validated info-hash extraction | 96.45% (136/141) / 100% (15/15) |
| `eh_ffi::webp` | Animated decoder ownership, callback/handle isolation, metadata, and reset behavior | 99.63% (271/272) / 100% (38/38) |
| `eh_archive::reader` | Streaming classic/ZIP64 indexing, declared/actual count bounds, natural ordering, extraction integrity, and opaque-handle lifecycle | 97.93% (899/918) / 100% (82/82) |
| `eh_archive::writer` | Sequential generated-CBZ naming, Stored output, summaries, and complete failure reporting | 100% (227/227) / 100% (24/24) |
| `eh_archive` | Generated-CBZ writing/validation plus safe ZIP/CBZ reading | 98.47% (1350/1371) / 100% (127/127) |
| `eh_net` aggregate | All currently migrated network behavior | 98.98% (5436/5492) / 99.12% (560/565) |

CI filters source and test paths for each critical module so an aggregate cannot
hide a regression. Rust owns only sanitized session-lifecycle planning;
`CookieManager`, credentials, WebView, HTTP, profile requests, and state commits
remain Android-owned.
Gallery Detail has separate preview, comment, and rich aggregate gates. Reader
page metadata has its own network gate, while `eh_image` independently gates
host-safe pixel, border, QR, and region-copy algorithms. `eh_ffi::health` has a
separate 95% gate and LCOV artifact so codec or WebP tests cannot mask it.
`eh_ffi::webp` now has
separate 95% evidence, exact host/final symbol manifests, and a real device contract. The
Gallery List/favorites request builders, the sanitized incoming list URL parser,
and Gallery Navigation each have an isolated network gate so URL-contract tests
cannot mask parser regressions. Navigation coverage includes both strict and loose
grammars, supported host/path variants, numeric and length bounds, stable CBOR,
Kotlin parity selection, the real JNI MVP chain, and post-profile-disable legacy
selection; active/physical rollback remains open.
Favorite Mutation now has its own isolated planner gate for exact single/bulk URL,
referer, origin, form ordering, CBOR goldens, and legacy-first selection; its
pre-HTTP plan equality cannot be masked by other Gallery request or response tests.
Gallery Archive Request Planning has a separate isolated planner gate for list and
download URL/header/origin/form semantics, exact request/response goldens, and
legacy-first whole-plan parity before funds lookup or HTTP. Android retains funds,
HTTP, the direct-download pending retry, response parsing, file effects, and UI; the
real API-35 JNI contract is compiled but remains unexecuted on this host.
Gallery Favorites response parsing has a separate gate for ordinal-10 category
skipping, strict signed counts, XML-decoded names, per-row drops, exact ten-pair
validation, shared Gallery List typed errors, the Rust/Kotlin golden, and default-off
single-parser selection. Android still owns GET/mutation HTTP, local favorites,
Room enrichment, paging, refresh, and UI; real API/device rollback evidence remains
open.
The API 35 image-device CI job now owns Bitmap/AHB format, repeated-lock, invalid
coordinate, hardware-bitmap wrapping, and Java buffer-close integration. A
separate Download Queue gate owns the pure reducer/recovery proof, while Kotlin
tests cover Android-state mapping, orphan recovery selection, envelope/effect
validation, native callback selection, and Rust-generated request/response CBOR
fixtures. Reader scheduling has its own pure-domain gate plus Kotlin request,
response, action-order, malformed-plan, and native callback contract tests; real
cache/image/network effects and rollback still need Android device evidence.
Reader Fetch has a separate production-only gate whose LCOV contains only
`reader/fetch.rs`. Its historical tests cover the bounded transition table and wire
contract. C7 must replace initial-fallback expectations with global-off Rust
zero-calls, global-on legacy zero-calls, strict failure, and stable operation
snapshots. Real-JNI sources compile, while API 35 execution, gallery/archive fetch
effects, cancellation, performance, and live rollback remain open.
Reader MPV has an isolated network gate plus marker/JSON/ordering, sanitized
failure, Rust/Kotlin golden, legacy-bug, and one-request parser-selection tests;
real token/page ordering, preview fallback, performance, and rollback still need
Android device evidence. Gallery Token has a separate gate for complete-list
decoding, first-item selection, ignored response gid, opaque/empty token values,
fixed token-free failures, Rust/Kotlin golden compatibility, and one-request
parser selection; real API/navigation and rollback still need Android device
evidence. Gallery Tag Vote has a combined isolated gate for its JSON envelope and
shared tag-row grammar; it covers source-order/alias-stripping/entity mapping, distinct Detail
versus Vote empty-text/marker rules, fixed failures, the Rust/Kotlin golden, and
default-off no-replay selection. Existing gallery gates exclude both shared files so
cross-module tests cannot mask regressions; real API/UI and rollback still need
Android device evidence. Gallery Rate has its own response-only gate for unknown and
duplicate fields, quoted numerics, full signed-count/finite-float compatibility,
fixed body-free
failures, Rust/Kotlin golden decoding, and default-off no-replay selection. Android
still owns credentials, request rounding, HTTP, and UI mutation; real API/device
rollback evidence remains open. Gallery Metadata has its own response-only gate for
full-list validation, duplicate-field/ID order, exact known-entity/category/thumbnail
normalization, fixed body-free failures, Rust/Kotlin golden decoding, and default-off
single-request selection. Android still owns request batching, matching, date and
language presentation, and mutation; real API/device rollback evidence remains open.
Gallery Comment Vote has its own response-only gate
for required signed widths, quoted integers, unknown and final duplicate fields,
unrelated service
errors, fixed body-free failures, Rust/Kotlin golden decoding, and default-off
single-request selection. Android still owns credentials, submitted votes, HTTP,
comment refresh, and UI; real API/device rollback evidence remains open. The
session-failure policy also has an independent gate and Kotlin
golden/precedence/forged-plan tests. Session lifecycle has a separate 100% domain
gate, Rust/Kotlin CBOR goldens, fake-store concurrency and rollback tests, and a
compiled API 35 native device contract. `CookieManager`, WebView, response bodies,
credentials, profile/configuration HTTP, live device behavior, and remote rollback
evidence remain Android-owned/open. The
remote archive parser has an isolated network gate, list/target CBOR goldens, and
Kotlin selection/security tests; real final-host downloads and rollback still
need device evidence. Image Limits has its own network gate, Rust/Kotlin CBOR
golden, sentinel tests, and default-off selection tests; real home/funds/reset
composition and rollback still need Android evidence. News has its own network gate
for missing/empty/nested panes, body-free failures, and the Rust/Kotlin golden;
Kotlin tests cover envelope validation, typed failure, cancellation, and default-off
single-parser selection. Its shared extractor is also exercised by Gallery Detail,
while HTTP, Cookie, scheduling, notifications, device execution, and rollback remain
Android-owned/open. User Config has an isolated
gate plus strict-shape, partial-success, golden, and parser-selection tests; the
redirect retry, settings mutation, and rollback still need Android evidence.
Torrent List has an isolated gate plus legacy grammar/skip, unsafe filename,
Rust/Kotlin golden, legacy CBOR, and parser-selection tests; real endpoint/tracker,
magnet intent/clipboard behavior, device performance, and rollback still need
Android evidence. The
archive writer and reader have independent gates plus an aggregate crate gate.
Writer tests cover pre-output contract validation, sequential non-empty Stored
entries, permissions, non-seekable output, input/output/finalization failures,
size overflow, and validator round trips. Kotlin tests cover exact native inputs,
invalid summaries, single-writer selection, and no fallback after Rust failure.
Android instrumentation compiles locally and runs real descriptor write/validate/
cleanup behavior in the pinned emulator CI job.
The unified reader acceptance must use real descriptor fixtures for ZIP/CBZ,
RAR/RAR5, 7z and tar; reject a wrong encrypted-ZIP password and accept the correct
one; cover direct-buffer and descriptor extraction, source-offset restoration,
CRC/truncation containment, active-close waiting, and idempotent close. The older
rawzip counts remain historical characterization only. A fresh C7 host/build/ABI
run is required; SAF, GIF decode, concurrent page loads, performance, broader
download E2E, and physical archive E2E remain deferred device evidence.

### Download Progress gates

After the final source edit, clean the coverage workspace once, then run the queue,
progress, and aggregate production denominators independently:

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/download/tests\.rs$|/download/progress/tests\.rs$|/download/progress\.rs$|/reader/|/reader\.rs$|/session/|/session\.rs$|/contracts\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- download::tests::
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/download/tests\.rs$|/download/progress/tests\.rs$|/download\.rs$|/reader/|/reader\.rs$|/session/|/session\.rs$|/contracts\.rs$|/lib\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95 -- download::progress::tests::
cargo llvm-cov --package eh_domain --all-features \
  --ignore-filename-regex '(/download/tests\.rs$|/download/progress/tests\.rs$|/reader/fetch/tests\.rs$)' \
  --fail-under-lines 95 --fail-under-functions 95
```

The pre-C6 queue production result was 97.05% line (329/339) and 100% function
(28/28). Progress production is 97.26% line (71/73) and 100% function (2/2).
Aggregate `eh_domain` is 99.41% line (2176/2189) and 100% function (116/116).
These counts are historical and C7 must refresh them. The three test files are
excluded from the aggregate production denominator;
queue and progress also exclude one another and the complete reader subtree, so
no slice can mask another. The host JVM manifest is sorted and exact at 44/44.
API35 runtime evidence is UNRUN and must
not be reported as a device pass.

### Reader Fetch gates

Reader Fetch has an independent production-only artifact and cannot borrow
coverage from Reader Schedule, Session, or its own separate test source:

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --package eh_domain --lib \
  --ignore-filename-regex '(/contracts\.rs$|/download/|/download\.rs$|/reader\.rs$|/reader/fetch/tests\.rs$|/session/|/session\.rs$|/lib\.rs$)' \
  --lcov --output-path target/llvm-cov/eh-domain-reader-fetch.lcov \
  --fail-under-lines 95 --fail-under-functions 95 -- reader::fetch::tests::
```

The Reader Schedule lane excludes `reader/fetch.rs` and
`reader/fetch/tests.rs` plus non-reader production sources. Session Failure and
Session Lifecycle exclude the whole `reader/` subtree. The aggregate domain lane
excludes `reader/fetch/tests.rs` while retaining `reader/fetch.rs`.

The pre-C6 evidence was Reader Fetch 201/201 lines and 7/7 functions, Reader
Schedule 412/412 and 22/22, Session Failure 144/144 and 8/8, Session Lifecycle
215/215 and 14/14, and aggregate domain 2176/2189 and 116/116. The Reader Fetch
LCOV has exactly one source record, `crates/domain/src/reader/fetch.rs`, with no
uncovered production line or function.

Those focused counts and the 44/44 V1 ABI are historical and must be refreshed from
the current mainline. `ReaderFetchDeviceTest` compiles but API 35 execution is UNRUN.
Reader Fetch invokes Rust directly. The bridge/test files were Google
Style clean at the recorded slice; the accepted narrow formatting deviation is the unrelated
whole-file rewrite that full Spotless requests for legacy `SpiderQueen.kt`.
