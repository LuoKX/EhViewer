# P2 portable-owner follow-up

Production owners:

- `eh_domain::reader_policy`: memory cache/entry accounting, decode concurrency and retry budget, SpiderQueen counters/completion, exact quota markers, advertisements and speed preferences.
- `eh_domain::transfer_speed`: rolling chunk retention and speed arithmetic; JNI keeps a leased mutex-protected native handle. Kotlin supplies monotonic elapsed nanoseconds and owns coroutine timers/HTTP cancellation.
- `eh_domain::recovery_search`: three-second request spacing, nonempty-result caching and 128-entry FIFO eviction. Kotlin retains gallery models, delay/HTTP, cache storage and synchronization.
- `eh_domain::spider_info`: complete CBOR storage map and legacy-v2 decoding; recovery identity extraction shares the codec while ignoring unrelated token-map fields as before.
- `eh_domain::local_pages`: eight-digit filename rules and stable directory selection. Reader preserves uppercase extensions and zero ordinals; recovery preserves lowercase positive ordinals. Android owns enumeration and file access.
- `eh_net::cache_keys`: image, thumbnail, timed-preview keys and site-specific URL restoration.

No selectors, shadow calculation, Kotlin business fallback or native retry are added.
All bridge results are validated before platform effects. OOM handling consumes a
precomputed Rust budget and makes no CBOR/JNI allocation. Cache entry sizing uses
scalar JNI. Image objects, SieveCache ownership, cancellation and Android exceptions
remain Kotlin-owned.

## Compatibility and bounded retention

SpiderInfo storage remains an unversioned map, compatible with existing Kotlin
CBOR files (including indefinite maps, omitted defaults and unknown fields). The full
page-token map and preview size now round-trip through Rust. Legacy duplicate rows
are last-value-wins; invalid numeric rows stop further row parsing. File reads and
native storage snapshots are capped at 512 KiB. Current cache entries do not enable
the legacy file grammar. Reader corruption returns no metadata; JNI/linkage/contract
failures surface and never activate a Kotlin parser.

Transfer windows preserve same-timestamp overwrite, inclusive cutoff and a ten-ms
minimum. Byte summation uses Long rather than the former overflowing Int. Android
now supplies a monotonic clock rather than wall time. Expired measurements are
pruned while tracking even when no consumer collects the flow; one window retains
at most 65,536 samples. Per-request handles close in finally; the download manager
owns its one process-lifetime tracker and resets it between downloads.

## Build defaults

Default and Marshmallow builds select only `arm64-v8a` for Debug and Release.
Additional architectures require an explicit `-PtargetAbis=x86_64` or
`-PtargetAbis=arm64-v8a,x86_64,armeabi-v7a`. Unsupported or empty ABI values fail
configuration. A universal APK is produced only for an explicit multi-ABI Release
build. CI release artifacts remain ARM64-only; emulator and baseline-profile
jobs explicitly select x86_64.

## Host evidence

Independent `python3 scripts/check_p2_coverage.py` gates exclude all other owners
and test sources and check one production denominator per run:

| Owner | Lines | Functions |
| --- | --- | --- |
| Reader policy | 91/91 | 8/8 |
| Transfer speed | 56/56 | 8/8 |
| Recovery search | 15/15 | 1/1 |
| SpiderInfo | 94/94 | 15/15 |
| Local pages | 48/48 | 9/9 |
| Cache keys | 51/52 | 7/7 |

Shared request/response CBOR goldens freeze all buffer-based contracts. Scalar
handle tests freeze creation, retirement, leases, invalid commands and close
idempotence. Kotlin tests freeze result validation, single-call routing, cancellation,
first/second OOM behavior and effect ordering. Isolated staged-tree acceptance:
420 Rust tests/doctests and 252 Kotlin unit tests pass, without unrelated local edits.
Default and Marshmallow Kotlin and offline Android test sources compile.
The default Debug APK contains only `arm64-v8a` and matches all 76/76 JNI V1 symbols.
Default ARM64 Release and explicit x86_64 task plans pass; invalid and empty ABI
parameters are rejected. Release APK execution and device testing remain unrun.

`P2MigrationDeviceTest` provides offline real-JNI probes for all owners.
Device execution and real reader memory/latency measurements remain UNRUN.
