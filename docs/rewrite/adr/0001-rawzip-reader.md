# ADR 0001: Stream ZIP Metadata with `rawzip`

- Status: Accepted
- Date: 2026-08-03

## Context

The transitional Rust ZIP/CBZ reader must reject archives with more than 100,000
central-directory entries before allocating metadata proportional to the declared
count. The existing `zip` 8.6 reader constructs its complete entry metadata vector
before the application can inspect `len()`, so a post-construction limit is a
behavioral bound but not a memory-allocation bound.

The reader must also preserve ZIP64, Stored/Deflate extraction, CRC verification,
legacy natural ordering, source descriptor offset restoration, and the existing
default-off rollback path. Implementing a second application-owned EOCD/ZIP64 parser
would duplicate security-sensitive format logic and create disagreement risks with
the extraction parser.

## Decision

Production indexing and extraction in `eh_archive::reader` use exactly pinned
`rawzip = 0.5.1`. The generated-CBZ writer, generated-CBZ validator, and test
fixture builder continue to use `zip` where its eager archive representation does
not create this external-reader allocation risk.

The reader applies the following policy:

1. Locate classic EOCD or ZIP64 through `rawzip::ZipArchive::from_seekable` with a
   fixed `MAX_CENTRAL_DIRECTORY_RECORD_SIZE` buffer (about 192 KiB).
2. Reject an `entries_hint()` above 100,000 before central-directory iteration.
3. Stream records without reserving from the untrusted count, enforce the same
   limit on the observed count, and require observed and declared counts to match.
4. Retain metadata only for playable entries: raw name bytes, a wayfinder,
   compression, extension, and bounded uncompressed size.
5. Reopen a bounded archive view for extraction, decode only Stored or Deflate,
   and read through `rawzip`'s CRC/size verifier before reporting success.

The exact pin also freezes the dependency's bounded default EOCD search behavior.
Any dependency upgrade requires rerunning the count-mismatch, ZIP64, CRC, coverage,
Android-target, and descriptor contract gates before changing the lockfile.

## Alternatives Considered

- Keep the `zip` post-construction `len()` check: rejected because untrusted entry
  count still influences allocation before policy enforcement.
- Add a local EOCD/ZIP64 preflight parser: rejected because fixed-tail shortcuts can
  misidentify comments/signatures, while a complete parser duplicates complex
  format and ZIP64 validation.
- Fork `zip` to expose an allocation limit: viable but rejected for this slice due
  to long-term patch maintenance and slower upstream security updates.

## Consequences

- External ZIP metadata is processed with constant record-buffer memory plus the
  bounded playable-entry index; no full metadata vector is sized from the declared
  entry count.
- Classic and ZIP64 declarations are checked against the records actually streamed.
- The archive crate temporarily carries both `rawzip` and `zip`, with distinct and
  documented responsibilities.
- Raw filenames remain byte strings during filtering and sorting, so invalid UTF-8
  does not force lossy conversion or alter the legacy lowercase extension policy.
- Password/encrypted ZIP, unsupported compression, RAR, 7z, and tar remain outside
  this reader and continue through the legacy native path.

## Migration and Rollback

JNI symbols, opaque-handle behavior, Kotlin adapter shape, and the
`useRustCoreArchiveReader` default remain unchanged. Rust-open failure may select
the legacy reader only before a Rust handle exists; extraction failure is never
replayed. Disabling the setting restores the legacy reader without schema or file
changes.
