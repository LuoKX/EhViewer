# P3 remaining portable rules and native dependency retirement

## Ordered stages

1. Filename rules and production text-range normalization: implemented.
2. Replace remaining libarchive ZIP/TAR and gzip/xz paths, then retire libarchive,
   liblzma and nettle: implemented; stage 2a/2b below retain historical evidence.
3. Replace libwebp/webpdemux decoding: implemented after host animation parity.
   Real ARM64 memory/performance and device acceptance remain UNRUN.
4. Evaluate HTTP execution, database access and other image codecs separately.
   No transport, credential, database-engine or Android platform change is made
   by the first stage.

RAR and 7z already use rars and sevenz-rust2. Stage 2 adds production ZIP reading
without changing the historical bounded ZIP V1 characterization or CBZ writing.

BBCode editing/tag emission follow-up lives in `p4-migration.md`. Stage notes
below describe their original acceptance boundary, not the current Kotlin owner.

## Stage 1 production owners

- eh_domain::file_rules owns FAT-invalid characters, trailing ASCII space/dot
  trimming, the 200-byte UTF-8 filename budget, center shortening, basename and
  last-dot extension extraction. FileUtils is now a direct adapter.
- eh_domain::text_spans owns overlap/adjacency union and ordering independently
  for bold, italic, underline and strikethrough. normalizeSpan sends all observed
  styles in one batch; comment text and Compose objects never enter JNI.
- Kotlin retains filesystem effects, title/settings observations, Compose style
  mapping, text editing and BBCode tag emission.
- The unused generic RangeSet/IntRangeSet APIs, including unused subtraction and
  intersection operations, are deleted rather than replaced with unused Rust
  abstractions. Production only used union.

Both owners have direct V1 JNI/CBOR contracts, shared request/response hex goldens
and strict result validation before platform effects. No selector, shadow
calculation, native retry or Kotlin business fallback is introduced. Empty
sanitized names retain IllegalStateException("Invalid filename"); transport,
linkage and other domain failures remain observable.

## Compatibility and budgets

Filename input/result strings use UTF-16 unit arrays, not lossy UTF-8 CBOR text.
This preserves lone Java surrogates and the original middle-deletion behavior.
Byte accounting uses Java UTF-8's one-byte replacement for lone surrogates.
Prefix/suffix tables make shortening linear while preserving the original
quadratic loop exactly; offline tests compare mixed Unicode and malformed UTF-16
cases against the frozen algorithm.

Filename requests are capped at 65,536 UTF-16 units. Sanitized results are checked
against the 200-byte budget and cannot contain path separators or NUL.
Extraction keeps null/empty/hidden-file, extension case and last-dot semantics.
Text normalization accepts at most 100,000 spans and Int-range UTF-16 endpoints;
results contain at most four entries per observation and must have valid bounds,
recognized single styles, style-group order and disjoint nonadjacent runs.

## Evidence

Local host acceptance: 430 Rust tests/doctests and 259 Kotlin JVM tests pass.
The JVM total includes pre-existing ComicInfo worktree tests, which this stage
does not modify. Rustfmt, host/Android Clippy, Rustdoc and naming checks pass.
Default and Marshmallow Kotlin compile; offline instrumentation sources compile.

Independent scripts/check_p3_coverage.py gates measure exactly one production
owner each, with every other owner and test source excluded:

| Owner | Lines | Functions |
| --- | --- | --- |
| File rules | 102/102 | 15/15 |
| Text spans | 39/39 | 4/4 |

CI includes the independent gates and excludes both owners/tests from older
per-slice denominators. Retired-source checks reject both Kotlin range files.
Host V1 exports match 78/78. The rebuilt default ARM64 Release APK contains only
arm64-v8a; all 101 Android Java JNI exports match and JNI_OnLoad is retained.
Fresh Debug configuration was blocked by Git TLS failure while fetching the
unchanged pinned xz dependency; Release built using its existing dependency cache,
without disabling certificate validation.

P3MigrationDeviceTest covers real filename and range JNI calls offline. Device
execution, full UI/reader/download E2E and ARM64 performance measurements remain
UNRUN. APK construction and instrumentation compilation are not runtime evidence.

## Stage 2a: ordinary ZIP/CBZ production decoding

eh_archive::production_zip now owns header inspection, password verification and
streamed ZIP decoding. The V2 registry keeps handle/password lifecycle, playable
filtering, natural ordering and recovery metadata selection. No JNI signature,
Kotlin platform execution, runtime selector or error-driven C fallback changes.

Content signatures and seekable footers identify ZIP, including self-extracting
prefixes. Physical indexes preserve duplicate filenames and Unicode-path collisions.
Filename interpretation keeps Android's UTF-8 default even without the ZIP UTF-8
bit, honors CRC-valid Info-ZIP Unicode paths and retains NUL termination.

ZipCrypto and AES password acceptance drains an encrypted regular member through
CRC/authentication; wrong passwords return false. Unsupported codecs and resource
failures remain errors, not password retries or backend switches. Extraction checks
member identity, declared length, CRC/MAC and output failures, and restores source
descriptor position on success/failure.

Before codec metadata allocation, preflight limits physical entries to 100,000
and directory/ZIP64 auxiliary bytes to 32 MiB. Decoding uses a 64 KiB copy buffer,
512 MiB per member, 64 MiB per LZMA/XZ dictionary or PPMd model, and 100,000 XZ
index records. Recovery metadata keeps its existing 512 KiB limit.
First-party eh_codec_limits supplies nested thread-local scopes and restores
prior limits on unwind; default RAR/7z decoding remains unchanged.

Pinned Rust sources and narrow local hooks are documented in
app/src/main/rust/vendor/README.md. ZIP deduplication is opt-out only for this
production reader; CBZ writer/validator default configuration remains unchanged.

Host fixtures cover Stored, Deflate, BZip2, LZMA, XZ, PPMd, ZipCrypto, AES,
MAC/CRC corruption, UTF-8/Unicode-path extras, hidden/NUL names, duplicates,
ZIP64 local headers, stream data descriptors, self-extracting prefixes, directory/
link/special-file exclusion, source/output failures and pre-allocation limits.
Deflate64 and legacy ZIP decoders are enabled but dedicated interoperability
fixtures are still pending. Real encrypted Android/SAF execution and ARM64
latency/peak-memory measurement remain UNRUN.

Independent production coverage: ZIP 154/161 lines, 16/16 functions; resource
policy 38/38 lines, 8/8 functions. Every other owner and test source is excluded.
Older slice denominators exclude both owners and vendored dependency source.

Local stage-2a acceptance: 455 Rust tests/doctests and a rerun of 259 Kotlin JVM
tests pass. Rustfmt, host/Android Clippy, Rustdoc and naming checks pass. Four
independent P3 coverage gates pass. Host V1 exports match 78/78; the ARM64 Release
APK retains JNI_OnLoad and all 101 Android Java exports, with arm64-v8a only.
Actual device/SAF execution is not inferred from building or symbol checks.
Outer TAR and gzip/xz signatures take priority over ZIP footers inside members;
checked-in nested-TAR and stored-gzip fixtures guard this format-detection boundary.

At stage 2a the C chain remained for TAR extended/sparse/PAX and outer gzip/xz,
including compressed ZIP. Stage 2b below replaces these remaining paths.
WebP, HTTP execution, database engines and Android filesystem effects are untouched.

## Next-stage compatibility gates

Host stage-2b gates cover the retired C archive chain with Rust production
ZIP encryption, filename encoding, ZIP64/data descriptors, TAR extended metadata,
gzip/xz filters, descriptor-position restoration, entry/order/CRC semantics and
bounded malformed-input handling. Keep RAR/7z behavior unchanged.

WebP replacement must preserve premultiplied alpha, frame composition/disposal,
loop counts/timing, reset/close behavior and malformed-input bounds. Compare real
ARM64 decode latency and peak memory before claiming a performance improvement.
## Stage 2b: TAR and outer gzip/XZ production decoding

production_tar owns V7/USTAR headers (including legacy USTAR versions), signed
checksums, GNU long names/links, PAX local/global/size/path overrides and deletion,
GNU old sparse extended headers, GNU PAX sparse 0.0/0.1/1.0, SCHILY real-size and
Solaris in-place holes. It never unpacks paths or follows links. Physical member
indexes preserve duplicates. Extraction checks identity and reconstructs holes
with a fixed zero buffer; invalid maps and unsupported versions fail explicitly.

Extended records/in-band maps are bounded to 512 KiB; visible members and sparse
map records to 100,000; headers to 1,000,000. A 32-MiB cumulative budget accounts
metadata payloads, owned PAX slots, decoded names and sparse tables before retaining
them. Global PAX uses BTreeMap, not quadratic replacement in a linear list.

archive_filters provides seekable fixed-buffer views without whole-container
allocation or temporary paths. Content sniffing handles short reads. Full initial
stream traversal verifies every gzip/XZ trailer, including concatenated streams,
before TAR/ZIP plans become visible. Backward seeks replay; this deliberately
trades CPU for bounded heap and avoids new Android storage effects.
Nesting is limited to 8; each layer to 64 GiB decoded; cumulative replay work to
256 GiB per view, including intermediate layers and prefix detection. Member and
recovery limits remain 512 MiB and 512 KiB.

Shared RAII allocation credit caps all simultaneously live LZ dictionaries and
PPMd models to 64 MiB across outer filters and inner ZIP decoding. Limits remain
scoped and unwind-safe; RAR/7z default behavior is unchanged. Unsupported resource
sizes are errors, not password retries or fallback switches.

The libarchive adapter, unsafe C structs/externs, libarchive/liblzma/nettle CMake
fetch/build/link steps and obsolete libarchive patches are removed. WebP C remains.
V1/V2 JNI signatures, Kotlin platform ownership and password/handle semantics are
unchanged. Existing user ComicInfo/download changes remain untouched.

Acceptance: 474 Rust tests/doctests, 259 rerun Kotlin JVM tests, 4 script tests;
host/Android Clippy, Rustfmt, Rustdoc and rewrite naming checks pass.
Independent coverage: TAR 477/501 lines and 46/46 functions; filters 155/155 and
19/19; decoder limits 86/86 and 18/18. All six P3 owner gates pass; vendored code
and other owners stay outside each denominator. ARM64 Release and instrumentation
source compilation pass. Host V1 matches 78/78; the arm64-v8a-only APK retains
JNI_OnLoad and all 101 Android Java exports, including every frozen V1 symbol.
C archive APIs are absent from APK dynamic exports and the build/link configuration.
Actual Android/SAF execution, large-archive replay latency and ARM64 peak memory
remain UNRUN. Existing dedicated Deflate64/legacy-ZIP interoperability gaps remain
listed under stage 2a; enabling codecs is not equivalent to testing those formats.

## Stage 3: pure-Rust animated WebP

eh_image::webp owns RIFF/ANMF indexing, owned input, codec-dimension preflight,
static-frame synthesis without copying each encoded payload, reusable RGBA raster,
premultiplied canvas, transparent disposal, source-over/replacement, reset and
cumulative JNI timestamps. ANIM background stays ignored, matching the former
libwebp decoder. Zero durations keep prior semantics: zero initial timestamp fails;
a later zero-duration frame keeps its positive cumulative timestamp. Finite loop
metadata and Kotlin playback ownership are unchanged.

Pinned image-webp 0.2.4 decodes individual VP8/VP8L/ALPH frames only. The upstream
nonpremultiplied animation compositor is not used. Premultiplication floors channel
times alpha /255; premultiplied source-over retains the prior integer /256 rule.
RGB output expands backward in the same reusable raster; no third persistent
canvas or whole-frame encoded copy is introduced.

Input is capped at 64 MiB, RGBA canvas/frame at 32 MiB (8 megapixels), and
frames/chunks at 100,000. Invalid embedded dimensions fail before codec allocation.
Each frame has a fresh conservative 64-MiB cumulative codec-allocation budget.
Narrow vendored hooks guard major raster/partition/alpha/transform/Huffman buffers
before allocation; scope defaults do not alter archive or RAR/7z behavior.
These application caps intentionally reject larger previously unbounded inputs.
Bounds are not a claim that arbitrary low-memory Android conditions cannot fail.

eh_ffi::webp remains an opaque synchronized handle adapter, now without C pointers,
unsafe Send, raw canvas slices or C deletion. Callback failure advances consumed
state as before; decode failure retains the previous composited canvas. JNI names,
Bitmap stride checks, positive metadata packing and idempotent close remain stable.
Both libwebp-sys2 dependencies and libwebp/webpdemux CMake fetch/build/link steps are
removed. Native C now consists only of the behavior-free link anchor; Android
Bitmap/HardwareBuffer/platform syscalls remain intentional platform interfaces.

Pinned libwebp 1.6.0 oracles are generated offline, not used at runtime. Six-frame
lossless and lossy/external-alpha fixtures match every premultiplied byte and
timestamp, covering offsets, finite loops, replacement/blend/disposal and background.
The existing ten-frame animation also matches every SHA-256 canvas golden.
Header/size/order/truncation/bit-flip, allocation budget, static metadata, reset,
owned input and callback error regressions pass. New Android instrumentation source
checks the real private JNI methods against raw premultiplied Bitmap bytes; its
compilation is not execution evidence.

Acceptance: 483 Rust tests/doctests and 259 Kotlin JVM tests pass. Eight independent
P3 gates pass: WebP image owner 353/358 lines and 26/26 functions; registry owner
207/208 and 36/36; decoder policy 108/108 and 22/22. Rustfmt, host/Android Clippy,
Rustdoc, naming checks, four script tests and Rust LSP checks pass. ARM64 Release
APK and Android instrumentation-source compilation pass. APK is ARM64-only and
retains JNI_OnLoad plus all 101 Java exports; frozen host V1 matches 78/78.
C codec dependencies are absent from Cargo.lock/CMake and APK dynamic exports.

True Android/SAF execution, ARM64 peak-memory/latency comparison and large-input
performance remain UNRUN. No mobile performance improvement is claimed. HTTP,
database engines, UI/platform effects and existing user download/ComicInfo work
remain untouched. Retained download/ComicInfo fixes are committed separately from
P3 migration for independent review and rollback.
