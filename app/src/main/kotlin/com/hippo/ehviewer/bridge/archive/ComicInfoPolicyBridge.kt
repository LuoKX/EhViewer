@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.archive

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class ComicInfoOperation {
  @SerialName("prepare") PREPARE,
  @SerialName("resolveExport") RESOLVE_EXPORT,
  @SerialName("write") WRITE,
}

@Serializable
internal enum class ComicInfoMetadataState {
  @SerialName("unread") UNREAD,
  @SerialName("missing") MISSING,
  @SerialName("present") PRESENT,
}

@Serializable
internal enum class ComicInfoAction {
  @SerialName("skip") SKIP,
  @SerialName("readMetadata") READ_METADATA,
  @SerialName("resolvePages") RESOLVE_PAGES,
  @SerialName("create") CREATE,
  @SerialName("rewrite") REWRITE,
  @SerialName("keep") KEEP,
}

@Serializable
internal enum class ComicInfoFetchMode {
  @SerialName("none") NONE,
  @SerialName("propagate") PROPAGATE,
  @SerialName("continue") CONTINUE,
}

/** Scalar observations only; XML, paths, titles and credentials remain outside this contract. */
@Serializable
internal data class ComicInfoPolicyRequest(
  @EncodeDefault val version: Int = 1,
  val operation: ComicInfoOperation,
  val hasDirectory: Boolean,
  val detailedGallery: Boolean,
  val fetchMetadata: Boolean,
  val galleryPages: Int,
  val expectedPages: Int?,
  val metadataState: ComicInfoMetadataState,
  val metadataPages: Int?,
) {
  companion object {
    fun prepare(expectedPages: Int, detailedGallery: Boolean, hasDirectory: Boolean) =
      ComicInfoPolicyRequest(
        operation = ComicInfoOperation.PREPARE,
        hasDirectory = hasDirectory,
        detailedGallery = detailedGallery,
        fetchMetadata = false,
        galleryPages = 0,
        expectedPages = expectedPages,
        metadataState = ComicInfoMetadataState.UNREAD,
        metadataPages = null,
      )

    fun resolveExport(galleryPages: Int, hasDirectory: Boolean) =
      ComicInfoPolicyRequest(
        operation = ComicInfoOperation.RESOLVE_EXPORT,
        hasDirectory = hasDirectory,
        detailedGallery = false,
        fetchMetadata = false,
        galleryPages = galleryPages,
        expectedPages = null,
        metadataState = ComicInfoMetadataState.UNREAD,
        metadataPages = null,
      )

    fun write(hasDirectory: Boolean, detailedGallery: Boolean, fetchMetadata: Boolean) =
      ComicInfoPolicyRequest(
        operation = ComicInfoOperation.WRITE,
        hasDirectory = hasDirectory,
        detailedGallery = detailedGallery,
        fetchMetadata = fetchMetadata,
        galleryPages = 0,
        expectedPages = null,
        metadataState = ComicInfoMetadataState.UNREAD,
        metadataPages = null,
      )
  }
}

@Serializable
internal data class ComicInfoPolicyPlan(
  val action: ComicInfoAction,
  val pageCount: Int?,
  val fetchMode: ComicInfoFetchMode,
)

/** Validate the complete plan before any platform effects. No retry or Kotlin policy fallback. */
internal fun planComicInfoPolicy(
  request: ComicInfoPolicyRequest,
  nativeCall: ComicInfoNativeCall = ::nativePlanComicInfoPolicyV1,
): ComicInfoPolicyPlan {
  require(
    request.version == 1 &&
      (request.metadataState == ComicInfoMetadataState.PRESENT) ==
        (request.metadataPages != null) &&
      (request.operation == ComicInfoOperation.PREPARE) == (request.expectedPages != null)
  ) {
    "Invalid ComicInfo policy input"
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size <= 4096) { "Invalid ComicInfo policy request size" }
  val envelope =
    invokeRustCbor<ApiEnvelope<ComicInfoPolicyPlan>>(encoded, "ComicInfo Policy", nativeCall)
  require(envelope.version == 1 && envelope.module == "comicInfoPolicy") {
    "Invalid ComicInfo policy envelope"
  }
  envelope.error?.let {
    if (
      it.kind == RustErrorKind.ERROR &&
        it.message in listOf(INVALID_ARCHIVE_PAGES, MISSING_DOWNLOAD_DIRECTORY)
    ) {
      throw IllegalArgumentException(it.message)
    }
    throw RustFailureException(it)
  }
  val plan = checkNotNull(envelope.payload) { "Missing ComicInfo policy payload" }
  validatePlan(request, plan)
  return plan
}

private fun validatePlan(request: ComicInfoPolicyRequest, plan: ComicInfoPolicyPlan) {
  if (plan.fetchMode != ComicInfoFetchMode.NONE) {
    require(
      plan.action == ComicInfoAction.CREATE &&
        request.hasDirectory &&
        !request.detailedGallery &&
        when (plan.fetchMode) {
          ComicInfoFetchMode.CONTINUE -> request.operation == ComicInfoOperation.PREPARE
          ComicInfoFetchMode.PROPAGATE ->
            request.operation == ComicInfoOperation.WRITE && request.fetchMetadata
          ComicInfoFetchMode.NONE -> false
        }
    ) {
      "Invalid ComicInfo fetch policy"
    }
  }
  require(
    when (plan.action) {
      ComicInfoAction.SKIP ->
        request.operation == ComicInfoOperation.WRITE &&
          !request.hasDirectory &&
          plan.pageCount == null &&
          plan.fetchMode == ComicInfoFetchMode.NONE
      ComicInfoAction.READ_METADATA ->
        request.operation != ComicInfoOperation.WRITE &&
          request.hasDirectory &&
          request.metadataState == ComicInfoMetadataState.UNREAD &&
          plan.pageCount == null &&
          plan.fetchMode == ComicInfoFetchMode.NONE &&
          (request.operation != ComicInfoOperation.RESOLVE_EXPORT || request.galleryPages <= 0)
      ComicInfoAction.RESOLVE_PAGES ->
        request.operation == ComicInfoOperation.RESOLVE_EXPORT &&
          plan.fetchMode == ComicInfoFetchMode.NONE &&
          plan.pageCount != null &&
          plan.pageCount > 0 &&
          (plan.pageCount == request.galleryPages || plan.pageCount == request.metadataPages)
      ComicInfoAction.CREATE ->
        request.hasDirectory &&
          when (request.operation) {
            ComicInfoOperation.PREPARE ->
              request.metadataState == ComicInfoMetadataState.MISSING &&
                plan.pageCount != null &&
                plan.pageCount > 0 &&
                plan.pageCount == request.expectedPages
            ComicInfoOperation.WRITE -> plan.pageCount == null
            ComicInfoOperation.RESOLVE_EXPORT -> false
          }
      ComicInfoAction.REWRITE,
      ComicInfoAction.KEEP ->
        request.operation == ComicInfoOperation.PREPARE &&
          request.hasDirectory &&
          request.metadataState == ComicInfoMetadataState.PRESENT &&
          plan.fetchMode == ComicInfoFetchMode.NONE &&
          plan.pageCount != null &&
          plan.pageCount > 0 &&
          plan.pageCount == request.expectedPages &&
          ((plan.action == ComicInfoAction.KEEP) == (plan.pageCount == request.metadataPages))
    }
  ) {
    "Invalid ComicInfo policy action"
  }
}

private external fun nativePlanComicInfoPolicyV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int

private const val INVALID_ARCHIVE_PAGES = "Archive expected page count must be positive"
private const val MISSING_DOWNLOAD_DIRECTORY = "Required value was null."
