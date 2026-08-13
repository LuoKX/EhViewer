# ADR 0002: Extract Versioned JNI Adapters into `eh_ffi`

- Status: Accepted
- Date: 2026-08-03

## Context

The target architecture requires one `eh_ffi` crate between the Android shell and
the platform-neutral Rust crates. Before this decision, every JNI export lived in
the legacy `ehviewer_rust` static-library source even after its business behavior
had moved into `eh_domain` or `eh_net`. That layout obscured dependency direction,
mixed versioned contracts with frozen rollback parsers, and left direct-buffer/CBOR
validation owned by a compatibility module.

A single move of all native code was not initially safe. Animated WebP, Android
Bitmap/HardwareBuffer access, generated-archive descriptors, and frozen legacy
parsers depended on root-only implementation modules or Android libraries. Each
extraction therefore follows an actual dependency boundary rather than an empty
target crate or a directory-only reshuffle.

## Decision

Create `crates/ffi` as `eh_ffi` and move every V1 JNI adapter that depends only on
`eh_domain` and `eh_net` into it. The initial extraction slice owned:

1. twenty response-parser exports backed by versioned `eh_net` envelopes;
2. seven request/plan exports for download, reader, session, and gallery URLs;
3. checked input-length, UTF-8, CBOR decode/encode, direct-buffer, and panic
   containment primitives;
4. an exact manifest of the 27 host-JVM V1 symbols present at that point.

The root static library remains the final native archive and directly depends on
`eh_ffi`. Its `jvm` feature forwards to `eh_ffi/jvm`. The existing CMake link uses
`--undefined-glob,Java_*` and the `Java_*` version-script export, so JNI functions in
the dependency crate retain the same final symbols without wrapper functions.
CI compares `nm` output against `crates/ffi/jvm-v1-symbols.txt` to make that linker
assumption executable. The device job diffs the exact V1 exports from the stripped
x86_64 `libehviewer.so` against the same manifest, covering the final
CMake/version-script link.

After the session-lifecycle planner added one request/plan export, a follow-up archive
slice adds nine writer, validator, and ZIP/CBZ reader descriptor exports backed only
by `eh_archive` plus POSIX descriptor duplication. The exact manifest therefore
contained 37 V1 symbols at that stage. Kotlin retains paths, SAF, lifecycle,
cleanup, and rollout decisions; `eh_ffi` owns only JNI validation, descriptor
duplication, direct-buffer access, and calls into the host-testable archive crate.

The Gallery Navigation follow-up adds the ninth request/plan export. The later News
event-pane slice adds the twenty-first response export; the then-current manifest
contains 39 V1 symbols: twenty-one response parsers, nine request or plan adapters,
and nine archive descriptor adapters. Favorite Mutation adds the tenth request/plan
export. Gallery Archive request planning adds the eleventh without moving HTTP, funds,
retry, response parsing, or download effects across JNI. Download Progress planning adds
the twelfth while Android retains workers, persistence, locks, and effects. The current
domain/network/archive subtotal is therefore 42 V1 symbols: twenty-one response
parsers, twelve request or plan adapters, and nine archive descriptor adapters.
The A1 Rust Core health foundation then adds one independent transport-diagnostic
export, bringing the current manifest to exactly 43 V1 symbols.

A second follow-up completes the image/WebP ownership slice. `eh_ffi::webp` owns the
libwebp decoder lifetime and opaque registry; `eh_ffi::jvm::webp` owns the three
host-JVM create/info/destroy exports; `eh_ffi::jvm::bitmap` owns border, QR, and frame
Bitmap adapters; and `eh_ffi::jvm::hardware_buffer` owns the API 26+ region-copy
adapter. Root forwards `jvm`, `android`, and `android-26` features but contains no
forwarding symbol, image module, WebP module, or direct `ndk`, `image`, `eh_image`,
or libwebp Cargo dependency. The three host WebP exports and nine final Android
image/WebP exports have separate exact manifests because host builds intentionally
exclude Android Bitmap symbols. The Android libwebp fork is bindings-only, so CMake
continues to build and supply `webp`/`webpdemux` at the final shared-library link;
that does not create a second Rust decoder or JNI owner.

The pure codec is unit-tested separately from JVM object access. Device-only JNI
glue is verified by Android target Clippy, Kotlin contract tests, symbol checks, and
instrumentation; it is not counted as host business logic merely to inflate unit
coverage. Rust Core health and WebP have independent source-filtered coverage gates,
so neither can borrow codec or aggregate tests.

The final cleanup on 2026-08-13 removed the frozen root parser tree, its fourteen
unversioned JNI exports, and its live-network test. It also removed the app-owned C
Archive wrapper, nine old Archive JNI exports, Kotlin declarations, natsort, private
header, and the earlier GIF C boundary. The root static library now retains only
`JNI_OnLoad` and the `eh_ffi` link edge. Exact positive and negative manifests make
that final ownership split executable in host and Android builds.

## Alternatives Considered

- Move all root FFI at once: rejected because legacy parsers and Android/WebP
  adapters would either create a dependency cycle or move business compatibility
  code into the target boundary.
- Keep JNI functions in root and add forwarding wrappers: rejected because the
  duplicate boundary would preserve split ownership and create symbol/argument
  drift risk.
- Add an empty `eh_ffi` crate for architectural completeness: rejected because an
  empty layer has no enforceable responsibility and makes the dependency graph
  less honest.

## Consequences

- New V1 parser and planner JNI code now follows `domain/net <- ffi <- Android`.
- Only `JNI_OnLoad` and static-link packaging remain in root; parser, animated WebP,
  GIF, archive, Bitmap, and HardwareBuffer JNI adapters have one owner in `eh_ffi`.
- Archive descriptor adapters now follow `ehviewer_rust -> eh_ffi -> eh_archive`;
  root has no direct `eh_archive` dependency or forwarding JNI wrapper.
- Image adapters follow `ehviewer_rust -> eh_ffi -> eh_image`, while libwebp state is
  private to `eh_ffi::webp`; root has no second registry or direct image/WebP Cargo
  dependency.
- Rust Core health follows `Android -> eh_ffi::jvm::health -> eh_ffi::health`, adds
  no startup invocation or rollout setting, and can be removed without touching
  `EhApplication` or `JNI_OnLoad`.
- `eh_ffi` documents every public adapter, denies Clippy warnings, and permits
  unsafe code only at local direct-buffer, descriptor, libwebp, Bitmap, and
  HardwareBuffer operations, each with an adjacent safety invariant and narrow lint
  allowance.
- The codec currently measures 100% line and function coverage (77/77 lines and
  7/7 functions; 97.99% regions).
- The health owner independently measures 100% line (74/74) and function (5/5)
  coverage.
- The WebP owner independently measures 99.63% line (271/272), 100% function (38/38),
  and 96.77% region (419/433) coverage.

## Migration and Rollback

The structural extraction did not change existing Kotlin declarations, JNI names,
CBOR fields, feature switches, or runtime fallbacks. The later health foundation adds
one new Kotlin/JNI/CBOR contract but does not alter an existing call or startup path.
Its rollback deletes the health owner, adapter, Kotlin bridge/tests, and independent
CI gate, then removes its single manifest line to restore 42/42. Application data and
rollout settings are unaffected. Future JNI extractions must extend the same crate
rather than create a second owner and must update the applicable exact host and
Android ABI manifests.
