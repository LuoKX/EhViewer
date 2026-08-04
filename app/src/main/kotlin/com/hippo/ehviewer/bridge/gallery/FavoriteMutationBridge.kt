package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Ordered form field; repeated names are intentionally preserved. */
internal typealias FavoriteMutationField = GalleryFormField

/** Single or bulk mutation operation. */
@Serializable
internal data class FavoriteMutationSingle(
  val gid: Long,
  val token: String,
  val destination: Int,
  val note: String,
)

/** Bulk move/delete payload consumed and planned by the Rust `galleryFavoriteMutation` V1 owner. */
@Serializable
internal data class FavoriteMutationBulk(val ids: List<Long>, val source: Int, val destination: Int)

/** Versioned request consumed by the pure Rust mutation planner. */
@Serializable
internal data class FavoriteMutationRequest(
  val version: Int,
  val site: GalleryUrlSite,
  val single: FavoriteMutationSingle? = null,
  val bulk: FavoriteMutationBulk? = null,
) {
  init {
    require((single != null) xor (bulk != null)) { "Invalid Gallery Favorites mutation operation" }
  }
}

/** Complete request plan selected before Android starts HTTP. */
@Serializable
internal data class FavoriteMutationPlan(
  val kind: FavoriteMutationKind,
  val url: String,
  val referer: String,
  val origin: String,
  val form: List<FavoriteMutationField>,
)

/** Operation discriminator matching the Rust `galleryFavoriteMutation` V1 owner. */
@Serializable
internal enum class FavoriteMutationKind {
  @SerialName("single") SINGLE,
  @SerialName("bulk") BULK,
}

/** Calls Rust once and validates the selected mutation kind. */
internal fun planFavoriteMutation(
  request: FavoriteMutationRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanFavoriteMutationV1,
): FavoriteMutationPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<FavoriteMutationPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Gallery Favorites mutation",
      nativeCall = nativeCall,
    )
  val expectedKind =
    if (request.single != null) FavoriteMutationKind.SINGLE else FavoriteMutationKind.BULK
  return envelope.toFavoriteMutationPlan(expectedKind)
}

internal fun ApiEnvelope<FavoriteMutationPlan>.toFavoriteMutationPlan(
  expectedKind: FavoriteMutationKind
): FavoriteMutationPlan {
  require(version == FAVORITE_MUTATION_API_VERSION) {
    "Unsupported Gallery Favorites mutation version"
  }
  require(module == FAVORITE_MUTATION_MODULE) { "Unexpected Gallery Favorites mutation module" }
  error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(payload) { "Missing Gallery Favorites mutation payload" }
  require(plan.kind == expectedKind) { "Gallery Favorites mutation kind mismatch" }
  return plan
}

internal fun selectFavoriteMutationPlan(
  legacyPlanner: () -> FavoriteMutationPlan,
  rustEnabled: () -> Boolean,
  rustPlanner: () -> FavoriteMutationPlan,
): FavoriteMutationPlan {
  val useRust = rustEnabled()
  return if (useRust) rustPlanner() else legacyPlanner()
}

/** Invokes the Rust V1 owner with separate direct request and response buffers. */
private external fun nativePlanFavoriteMutationV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val FAVORITE_MUTATION_API_VERSION = 1
internal const val FAVORITE_MUTATION_MODULE = "galleryFavoriteMutation"
