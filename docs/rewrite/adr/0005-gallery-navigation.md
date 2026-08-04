# ADR 0005: Gallery Navigation Parsing in Rust

## Status

Accepted for the staged Rust rewrite.

## Context

Gallery detail deep links and single-page reader links were still interpreted by
two Kotlin regex parsers. They are MVP entry points used by app links, clipboard
opening, list navigation, settings, and the reader. Keeping their host, path, token,
integer, and zero-based-page rules in Android left network protocol behavior outside
the Rust core and without deterministic coverage.

The existing `strict` mode is observable compatibility behavior rather than a full
URL parser: it uses a case-sensitive `Regex.find`, accepts matching substrings and
suffixes, and recognizes E/EX plus public `/lofi`. Loose detail parsing additionally
requires a non-hexadecimal boundary after the ten-character token. Candidate URLs
contain access tokens and therefore cannot appear in errors or logs.

## Decision

### One network-owned contract

`eh_net::gallery::navigation` owns both grammars behind one V1
`galleryNavigation` contract. A request contains:

- protocol `version`;
- closed `detail` or `page` target;
- the raw candidate string;
- the frozen strict/loose mode.

The response is a closed `none`, `detail`, or `page` plan. Matching plans contain a
signed gallery ID and ten-character lowercase hexadecimal token; page plans also
contain the zero-based page index. The module preserves strict substring/suffix,
`/g`, `/mpv`, `/s`, public `/lofi`, and loose token-boundary behavior. Numeric
overflow returns no match instead of throwing from Kotlin conversion. UTF-8 input is
bounded at 16 KiB. No error or log includes the candidate or token.

The only JNI implementation is
`eh_ffi::jvm::plan::nativeParseGalleryNavigationV1`. It uses separate direct CBOR
request/response buffers and extends the exact V1 ABI manifest to 38 symbols.

### Default-off parity rollout

`useRustCoreGalleryNavigation` remains false by default and belongs to the
coordinated MVP profile. `GalleryDetailUrlParser` and `GalleryPageUrlParser` remain
compatibility wrappers. They first run their side-effect-free frozen grammar; a no
match returns immediately without JNI. For a match, Kotlin validates the Rust
version, module, result shape, token, target, and complete equality with the legacy
plan before constructing a destination result. Boundary, linkage, input-length, or
parity failure returns the legacy result before navigation has started.

## Alternatives Considered

- Two Rust modules and JNI symbols were rejected because both parsers own one small
  navigation protocol and share the same token/ID/result invariants.
- Canonical URL parsing was deferred because it would silently change the existing
  substring and suffix behavior during a parity rollout.
- Pre-parsing every path component in Kotlin was rejected because Android would
  remain the actual protocol owner.
- Removing the Kotlin regexes immediately was rejected because system-wide
  verified-domain default routing, clipboard, positive-page network navigation,
  and rollback evidence is not yet complete.

## Consequences

- Detail and reader navigation protocol knowledge now has one short, host-testable
  Rust module with 98.80% line and 100% function coverage.
- Rust/Kotlin request and response CBOR bytes are fixed by shared golden tests.
- The API-35 MVP instrumentation chain calls both navigation kinds through the real
  native library. An additive Activity test verifies package-scoped manifest
  resolution for both gallery hosts and HTTP schemes, rejects an unknown host and
  FTP, launches an implicit page-zero `ACTION_VIEW`, follows the real intent channel
  into `ProgressScreen`, and uses the
  legacy `pageIndex = -1` sentinel to stop before HTTP.
- System-wide verified-domain default routing, clipboard dispatch, positive-page
  network navigation, performance, and physical rollback remain separate gates.
- Raw candidates cross the local JNI boundary by design but are length-bounded and
  prohibited from diagnostics. No network request or navigation side effect occurs
  in Rust.
