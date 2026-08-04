package com.hippo.ehviewer.bridge

import com.hippo.ehviewer.client.exception.EhError

/**
 * Carries a decoded Rust business failure from a bridge adapter to the shared HTTP error mapper.
 *
 * Transport and serialization failures must use their original exception instead of this wrapper.
 */
internal class RustFailureException(val failure: RustError) :
  Exception("Rust business failure: ${failure.kind}")

/** Converts the V1 Rust error contract to the existing Android business-error model. */
internal fun RustError.toLegacyError(): EhError =
  when (kind) {
    RustErrorKind.NO_HITS -> EhError.NoHits
    RustErrorKind.NO_WATCHED -> EhError.NoWatched
    RustErrorKind.NEED_LOGIN -> EhError.NeedLogin
    RustErrorKind.NO_HATH_CLIENT -> EhError.NoHathClient
    RustErrorKind.INSUFFICIENT_FUNDS -> EhError.InsufficientFunds
    RustErrorKind.GALLERY_UNAVAILABLE -> EhError.GalleryUnavailable(requireNotNull(message))
    RustErrorKind.IP_BANNED -> EhError.IpBanned(requireNotNull(message))
    RustErrorKind.ERROR -> EhError.Error(requireNotNull(message))
  }

/** Converts the legacy tuple-shaped parser error into the versioned Rust contract. */
internal fun EhError.toRustError(): RustError =
  when (this) {
    EhError.NoHits -> RustError(RustErrorKind.NO_HITS)
    EhError.NoWatched -> RustError(RustErrorKind.NO_WATCHED)
    EhError.NeedLogin -> RustError(RustErrorKind.NEED_LOGIN)
    EhError.NoHathClient -> RustError(RustErrorKind.NO_HATH_CLIENT)
    EhError.InsufficientFunds -> RustError(RustErrorKind.INSUFFICIENT_FUNDS)
    is EhError.GalleryUnavailable -> RustError(RustErrorKind.GALLERY_UNAVAILABLE, message)
    is EhError.IpBanned -> RustError(RustErrorKind.IP_BANNED, message)
    is EhError.Error -> RustError(RustErrorKind.ERROR, message)
  }
