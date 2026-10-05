# P1 portable-owner follow-up

Scope: tag dictionaries, EH recovery policy, Gallery request planning and language/
metadata inference. Reader memory/speed policies, SpiderInfo codec consolidation and
remaining local-file/cache-key rules are P2 and intentionally unchanged.

## Owners and production paths

| Rust owner | Android entry |
| --- | --- |
| `eh_net::tag_translation` | `EhTagDatabase` descriptor load, indexed lookup, bounded top-50 suggestions |
| `eh_domain::recovery_eh_lookup` | `DownloadRecoveryEhLookup` hints, lookup order, result selection, pre-HTTP identity validation, completeness/status/issues/evidence and failure precedence |
| `eh_net::gallery::request` | `EhEngine` comment/showpage/rating/gdata/votes/gtoken and `EhUrl` Detail/reader/MPV URL plans |
| `eh_domain::gallery_enrichment` | tag/title language inference and extra-metadata eligibility |

Kotlin owns HTTP, locale/settings snapshots, exception class observation, closed Room
failure-signal mapping, path checks, model assignment, Room transactions and promotion.
API user IDs, API keys, cookies and arbitrary Throwable messages never enter these
request/policy inputs. Native failures surface; no Kotlin business fallback is retained.

Recovery HTTP caches now retain raw observations. Rust applies each candidate's known
gid/title selection after cache lookup, preventing a same-title/different-gid request
from reusing another candidate's filtered choices. Network/NoHits failover remains
ordered and separate from native policy failures.

## Tag index ownership and budgets

The entire JSON/dictionary never enters the Java heap or the generic CBOR bridge.
Ktor streams at most 64 MiB to a staging file. JNI duplicates its descriptor, reads
from absolute offset zero without changing the shared cursor, parses in Rust and
publishes an immutable opaque handle only after successful parsing. Kotlin's read/
write publication lock protects complete operations; Rust clones an Arc lease before
releasing its registry lock. Retired handles close idempotently; active leases remain
valid. Failed candidates do not replace the existing index.

Existing Kotlin prefix-map cache files remain readable in Rust. New original-JSON
cache files use the `.rust-v1` suffix; data/SHA staging files are moved only after
successful index loading. Startup update-age behavior is retained.

Index keys are limited to 4 KiB, hints to 64 KiB and prefixes to 128 bytes. Queries
are limited to 16 KiB and fifty results. A stable bounded top-N avoids allocating
every matching row. Source order, last-value-wins tag/namespace duplicates, prefix
stripping, namespace weights, UTF-16 lengths and Float scores are pinned by tests.
Suggestions preserve untrimmed hints; single lookups retain Kotlin whitespace trimming.
Native loader failures use fixed, input-free diagnostics.

Tag suggestions use exact-sized, capped (4 MiB) byte-array CBOR responses. JSON request
plans also use exact-sized responses (16 MiB); escaped control characters can exceed
the old four-times-request buffer heuristic. API credentials are inserted by Android
immediately after the method field, preserving the old ordered JSON body.

## Evidence

`python3 scripts/check_p1_coverage.py` cleans profiles for each owner, excludes every
other production/test source and asserts exactly one measured production file.
Current independent line/function evidence:

| Owner | Lines | Functions |
| --- | --- | --- |
| Tag translation | 238/239 (99.58%) | 27/27 |
| Gallery request | 201/201 (100%) | 19/19 |
| Gallery enrichment | 60/60 (100%) | 10/10 |
| EH recovery | 353/353 (100%) | 39/39 |

Final local gate: 398 Rust unit tests plus three doctests (401 total), 252 Kotlin JVM
tests, Rustfmt, host/Android Clippy, Rustdoc and naming checks pass. Default and
Marshmallow Kotlin compile; instrumentation sources compile. Final arm64-v8a and
x86_64 debug shared libraries each match 67/67 V1 exports. Aggregate line/function
coverage is 99.20%/98.54% for `eh_domain` and 99.03%/99.22% for `eh_net`, excluding
the new standalone test sources.

Rust/Kotlin share checked-in request/response CBOR goldens. JVM tests freeze Ktor's URI
encoding and JSON field ordering; cover version/module/operation/row drift, invalid
indices/statuses, linkage/exception propagation and single-invocation routing. Host
FFI tests cover descriptor offsets, malformed/oversized input and idempotent close/
Arc leases. The exact V1 manifest grows from 60 to 67 exports.

`P1MigrationDeviceTest` invokes all four real JNI boundaries offline, including index
descriptor lifetime/offset and closed/version failures. Its local source compilation
is host evidence only. Device execution and memory/latency measurements remain UNRUN;
no full reader/download/network E2E pass is claimed.
